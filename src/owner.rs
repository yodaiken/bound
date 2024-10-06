use std::{
    collections::{HashMap, HashSet},
    io::{self, Cursor},
    path::PathBuf,
};

use crate::{read_file_at_commit, CommitInfo};

const CODEOWNERS_LOCATIONS: [&str; 3] = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

pub fn get_codeowners_at_commit(
    commit_id: &str,
    cwd: &PathBuf,
) -> Result<Option<String>, io::Error> {
    for location in CODEOWNERS_LOCATIONS.iter() {
        if let Some(content) = read_file_at_commit(commit_id, location, cwd)? {
            return Ok(Some(content));
        }
    }
    Ok(None)
}

pub struct CommitInfoWithCodeowner {
    pub id: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub file_changes: Vec<FileChangeWithCodeowner>,
}

pub struct FileChangeWithCodeowner {
    pub insertions: i32,
    pub deletions: i32,
    pub path: String,
    pub codeowners: Option<Vec<String>>,
    pub author_is_codeowner: Option<bool>,
}

pub struct AuthorCodeownerMemberships {
    pub author_email: Option<String>,
    pub author_name: Option<String>,
    pub codeowner: String,
}

struct AuthorMembership {
    email_to_codeowner: HashMap<String, HashSet<String>>,
    name_to_codeowner: HashMap<String, HashSet<String>>,
}

impl AuthorMembership {
    fn new(memberships: &[AuthorCodeownerMemberships]) -> Self {
        let mut email_to_codeowner = HashMap::new();
        let mut name_to_codeowner = HashMap::new();

        for membership in memberships {
            if let Some(email) = &membership.author_email {
                email_to_codeowner
                    .entry(email.clone())
                    .or_insert_with(HashSet::new)
                    .insert(membership.codeowner.clone());
            }
            if let Some(name) = &membership.author_name {
                name_to_codeowner
                    .entry(name.clone())
                    .or_insert_with(HashSet::new)
                    .insert(membership.codeowner.clone());
            }
        }

        Self {
            email_to_codeowner,
            name_to_codeowner,
        }
    }

    fn get_codeowners_for_author(&self, author_name: &str, author_email: &str) -> HashSet<String> {
        let mut codeowners = HashSet::new();
        if let Some(email_codeowners) = self.email_to_codeowner.get(author_email) {
            codeowners.extend(email_codeowners.iter().cloned());
        }
        if let Some(name_codeowners) = self.name_to_codeowner.get(author_name) {
            codeowners.extend(name_codeowners.iter().cloned());
        }
        codeowners
    }

    fn is_codeowner(&self, author_name: &str, author_email: &str, codeowner: &str) -> bool {
        self.get_codeowners_for_author(author_name, author_email)
            .contains(codeowner)
    }
}

pub struct CommitWithCodeownersIterator<I>
where
    I: Iterator<Item = Result<CommitInfo, io::Error>>,
{
    commit_iter: I,
    cwd: PathBuf,
    memberships: Option<AuthorMembership>,
    cached_owners: Option<codeowners::Owners>,
}

fn codeowners_changed(commit: &CommitInfo) -> bool {
    commit
        .file_changes
        .iter()
        .any(|change| CODEOWNERS_LOCATIONS.contains(&change.path.as_str()))
}

impl<I> Iterator for CommitWithCodeownersIterator<I>
where
    I: Iterator<Item = Result<CommitInfo, io::Error>>,
{
    type Item = Result<CommitInfoWithCodeowner, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let commit = match self.commit_iter.next()? {
            Ok(commit) => commit,
            Err(e) => return Some(Err(e)),
        };

        if self.cached_owners.is_none() || codeowners_changed(&commit) {
            match get_owners_at_commit(&commit.id, &self.cwd) {
                Ok(owners) => self.cached_owners = Some(owners),
                Err(e) => return Some(Err(e)),
            }
        }

        let owners = self.cached_owners.as_ref().unwrap();

        Some(Ok(CommitInfoWithCodeowner {
            id: commit.id,
            author_name: commit.author_name.clone(),
            author_email: commit.author_email.clone(),
            timestamp: commit.timestamp,
            file_changes: commit
                .file_changes
                .into_iter()
                .map(|change| {
                    let file_owners = owners.of(&change.path).map(|owners| {
                        owners
                            .iter()
                            .map(|o| o.to_string())
                            .collect::<Vec<String>>()
                    });

                    let author_name = &commit.author_name;
                    let author_email = &commit.author_email;

                    FileChangeWithCodeowner {
                        insertions: change.insertions,
                        deletions: change.deletions,
                        codeowners: file_owners.clone(),
                        author_is_codeowner: self.memberships.as_ref().map(|memberships| {
                            is_author_codeowner(
                                memberships,
                                &file_owners.clone().unwrap_or_default(),
                                author_name,
                                author_email,
                            )
                        }),
                        path: change.path,
                    }
                })
                .collect(),
        }))
    }
}

fn get_owners_at_commit(commit_id: &str, cwd: &PathBuf) -> Result<codeowners::Owners, io::Error> {
    let codeowners_str = get_codeowners_at_commit(commit_id, cwd)?;

    let reader = match codeowners_str {
        Some(content) => Cursor::new(content),
        None => Cursor::new("".to_owned()),
    };

    Ok(codeowners::from_reader(reader))
}

fn is_author_codeowner(
    memberships: &AuthorMembership,
    owners: &[String],
    commit_author_name: &str,
    commit_author_email: &str,
) -> bool {
    owners
        .iter()
        .any(|owner| memberships.is_codeowner(commit_author_name, commit_author_email, owner))
}

pub fn git_log_commits_with_codeowners(
    since: &str,
    until: &str,
    cwd: &PathBuf,
    memberships: Option<Vec<AuthorCodeownerMemberships>>,
) -> Result<impl Iterator<Item = Result<CommitInfoWithCodeowner, io::Error>>, io::Error> {
    let commit_iter = crate::git_log_commits(since, until, cwd)?;

    let author_membership = memberships.map(|m| AuthorMembership::new(&m));

    Ok(CommitWithCodeownersIterator {
        commit_iter,
        memberships: author_membership,
        cwd: cwd.clone(),
        cached_owners: None,
    })
}

use std::fs::File;
use std::io::{BufRead, BufReader, Write};

pub fn write_memberships_to_tsv(
    memberships: &[AuthorCodeownerMemberships],
    path: &PathBuf,
) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "author_email\tauthor_name\tcodeowner")?;
    for membership in memberships {
        writeln!(
            file,
            "{}\t{}\t{}",
            membership.author_email.as_deref().unwrap_or(""),
            membership.author_name.as_deref().unwrap_or(""),
            membership.codeowner
        )?;
    }
    Ok(())
}

pub fn read_memberships_from_tsv(path: &PathBuf) -> io::Result<Vec<AuthorCodeownerMemberships>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut memberships = Vec::new();

    let mut lines = reader.lines();

    // Skip the first line
    lines.next();

    for line in lines {
        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid line: {}", line),
            ));
        }
        memberships.push(AuthorCodeownerMemberships {
            author_email: if parts[0].is_empty() {
                None
            } else {
                Some(parts[0].to_string())
            },
            author_name: if parts[1].is_empty() {
                None
            } else {
                Some(parts[1].to_string())
            },
            codeowner: parts[2].to_string(),
        });
    }

    Ok(memberships)
}
