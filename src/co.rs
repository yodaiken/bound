use std::{path::Path, process::Command};

use chrono::{DateTime, Utc};

use crate::{git_log_commits, BoundError, CommitAuthor, CommitInfo};

const CODEOWNERS_LOCATIONS: [&str; 3] = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

fn read_file_at_commit(
    commit_id: &str,
    file_path: &str,
    cwd: &Path,
) -> Result<Option<String>, BoundError> {
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", commit_id, file_path)])
        .current_dir(cwd)
        .output()
        .map_err(BoundError::GitExecutionError)?;

    if output.status.success() {
        let content = String::from_utf8(output.stdout).map_err(BoundError::UTF8Error)?;
        Ok(Some(content))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.starts_with("fatal: path") {
            Ok(None)
        } else {
            Err(BoundError::GitExecutionError(std::io::Error::new(
                std::io::ErrorKind::Other,
                stderr,
            )))
        }
    }
}

pub fn get_codeowners_at_commit(commit_id: &str, cwd: &Path) -> Result<Option<String>, BoundError> {
    for location in CODEOWNERS_LOCATIONS.iter() {
        if let Some(content) = read_file_at_commit(commit_id, location, cwd)? {
            return Ok(Some(content));
        }
    }
    Ok(None)
}

#[derive(Debug, Clone)]
pub struct CommitInfoWithCodeowner {
    pub id: String,
    pub author: CommitAuthor,
    pub date: DateTime<Utc>,
    pub file_changes: Vec<FileChangeWithCodeowner>,
}

#[derive(Debug, Clone)]
pub struct FileChangeWithCodeowner {
    pub path: String,
    pub codeowners: Option<Vec<String>>,
    pub author_is_codeowner: Option<bool>,
    pub insertions: i32,
    pub deletions: i32,
}

#[derive(Debug, Clone)]
pub struct AuthorCodeownerMemberships<'a> {
    pub author_email: Option<&'a str>,
    pub author_name: Option<&'a str>,
    pub codeowner: &'a str,
}

impl AuthorCodeownerMemberships<'_> {
    pub fn author_matches(&self, author: &CommitAuthor) -> bool {
        (self.author_email.is_some()
            && self.author_email.unwrap().to_lowercase() == author.email.to_lowercase())
            || (self.author_name.is_some()
                && self.author_name.unwrap().to_lowercase() == author.name.to_lowercase())
    }
}

pub struct CommitsWithCodeownersIterator<'a, I> {
    commits: I,
    cwd: &'a Path,
    memberships: Option<&'a Vec<AuthorCodeownerMemberships<'a>>>,
}

impl<'a, I: Iterator<Item = Result<CommitInfo, BoundError>>> Iterator
    for CommitsWithCodeownersIterator<'a, I>
{
    type Item = Result<CommitInfoWithCodeowner, BoundError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.commits.next() {
            Some(Ok(commit)) => {
                let codeowners_contents = get_codeowners_at_commit(&commit.id, self.cwd)
                    .ok()
                    .flatten();
                let codeowners = codeowners_contents
                    .as_ref()
                    .map(|contents| codeowners::from_reader(contents.as_bytes()));

                let file_changes_with_codeowners: Vec<FileChangeWithCodeowner> = commit
                    .file_changes
                    .into_iter()
                    .map(|fc| {
                        let file_codeowners: Option<Vec<String>> = codeowners
                            .as_ref()
                            .and_then(|co| co.of(&fc.path))
                            .map(|owners| owners.iter().map(|owner| owner.to_string()).collect());
                        let author_is_codeowner = self.memberships.as_ref().map(|m| {
                            file_codeowners.as_ref().map_or(false, |owners| {
                                owners.iter().any(|owner| {
                                    m.iter().any(|membership| {
                                        membership.author_matches(&commit.author)
                                            && owner == membership.codeowner
                                    })
                                })
                            })
                        });

                        FileChangeWithCodeowner {
                            codeowners: file_codeowners,
                            author_is_codeowner,
                            path: fc.path,
                            insertions: fc.insertions,
                            deletions: fc.deletions,
                        }
                    })
                    .collect();

                Some(Ok(CommitInfoWithCodeowner {
                    id: commit.id,
                    author: commit.author,
                    date: commit.date,
                    file_changes: file_changes_with_codeowners,
                }))
            }
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }
}

pub fn git_log_commits_with_codeowners<'a>(
    since: &str,
    until: &str,
    cwd: &'a Path,
    memberships: Option<&'a Vec<AuthorCodeownerMemberships>>,
) -> Result<
    CommitsWithCodeownersIterator<'a, impl Iterator<Item = Result<CommitInfo, BoundError>> + 'a>,
    BoundError,
> {
    let commits = git_log_commits(since, until, cwd)?;
    Ok(CommitsWithCodeownersIterator {
        commits,
        cwd,
        memberships,
    })
}
