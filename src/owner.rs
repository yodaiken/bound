use std::{
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

impl AuthorCodeownerMemberships {
    pub fn author_matches(&self, author_name: &str, author_email: &str) -> bool {
        (self.author_email.is_some()
            && self.author_email.as_ref().unwrap().to_lowercase() == author_email.to_lowercase())
            || (self.author_name.is_some()
                && self.author_name.as_ref().unwrap().to_lowercase() == author_name.to_lowercase())
    }
}

pub struct CommitWithCodeownersIterator<I>
where
    I: Iterator<Item = Result<CommitInfo, io::Error>>,
{
    commit_iter: I,
    cwd: PathBuf,
    memberships: Option<Vec<AuthorCodeownerMemberships>>,
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

                    let memberships = self.memberships.as_ref();
                    let author_name = &commit.author_name;
                    let author_email = &commit.author_email;

                    FileChangeWithCodeowner {
                        insertions: change.insertions,
                        deletions: change.deletions,
                        codeowners: file_owners.clone(),
                        author_is_codeowner: memberships.map(|memberships| {
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
    memberships: &[AuthorCodeownerMemberships],
    owners: &[String],
    commit_author_name: &str,
    commit_author_email: &str,
) -> bool {
    owners.iter().any(|owner| {
        memberships
            .iter()
            .filter(|membership| &membership.codeowner == owner)
            .any(|membership| membership.author_matches(commit_author_name, commit_author_email))
    })
}

pub fn git_log_commits_with_codeowners(
    since: &str,
    until: &str,
    cwd: &PathBuf,
    memberships: Option<Vec<AuthorCodeownerMemberships>>,
) -> Result<impl Iterator<Item = Result<CommitInfoWithCodeowner, io::Error>>, io::Error> {
    let commit_iter = crate::git_log_commits(since, until, cwd)?;

    Ok(CommitWithCodeownersIterator {
        commit_iter,
        memberships,
        cwd: cwd.clone(),
        cached_owners: None,
    })
}
