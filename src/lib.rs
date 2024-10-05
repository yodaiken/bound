use chrono::{DateTime, TimeZone, Utc};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BoundError {
    #[error("Failed to execute git command: {0}")]
    GitExecutionError(#[from] std::io::Error),
    #[error("Failed to parse UTF-8: {0}")]
    UTF8Error(#[from] std::string::FromUtf8Error),
    #[error("Expected <EMPTY LINE>, got: {0}")]
    UnexpectedLineError(String),
    #[error("Expected COMMIT, got: {0}")]
    UnexpectedCommitError(String),
    #[error("Failed to parse CODEOWNERS file: {0}")]
    CodeownersParseError(String),
}

#[derive(Debug, Clone)]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub insertions: i32,
    pub deletions: i32,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub id: String,
    pub author: CommitAuthor,
    pub date: DateTime<Utc>,
    pub file_changes: Vec<FileChange>,
}

fn git_log_lines(
    since: &str,
    until: &str,
    cwd: &Path,
) -> Result<impl Iterator<Item = Result<String, BoundError>>, BoundError> {
    let output = Command::new("git")
        .args([
            "log",
            "--no-merges",
            "--format=COMMIT%n%H%n%at%n%an%n%ae",
            "--numstat",
            &format!("--since={}", since),
            &format!("--until={}", until),
        ])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(BoundError::GitExecutionError)?
        .stdout
        .ok_or_else(|| {
            BoundError::GitExecutionError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to capture stdout",
            ))
        })?;

    Ok(BufReader::new(output)
        .lines()
        .map(|l| l.map_err(BoundError::GitExecutionError)))
}

fn next_commit_info_from_git_log_lines<I: Iterator<Item = Result<String, BoundError>>>(
    lines: &mut I,
) -> Result<Option<CommitInfo>, BoundError> {
    let commit_id = match lines.next().transpose()? {
        Some(id) => id,
        None => return Ok(None),
    };
    let commit_date = Utc
        .timestamp_opt(
            lines
                .next()
                .transpose()?
                .ok_or(BoundError::UnexpectedCommitError(
                    "Missing timestamp".to_string(),
                ))?
                .parse()
                .map_err(|_| BoundError::UnexpectedCommitError("Invalid timestamp".to_string()))?,
            0,
        )
        .earliest()
        .ok_or(BoundError::UnexpectedCommitError(
            "Invalid timestamp".to_string(),
        ))?;
    let current_author_name =
        lines
            .next()
            .transpose()?
            .ok_or(BoundError::UnexpectedCommitError(
                "Missing author name".to_string(),
            ))?;
    let current_author_email =
        lines
            .next()
            .transpose()?
            .ok_or(BoundError::UnexpectedCommitError(
                "Missing author email".to_string(),
            ))?;

    if let Some(line) = lines.next().transpose()? {
        if !line.is_empty() {
            return Err(BoundError::UnexpectedLineError(line));
        }
    }

    let mut file_changes = Vec::new();

    while let Some(line) = lines.next().transpose()? {
        if line == "COMMIT" {
            break;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() == 3 {
            file_changes.push(FileChange {
                path: parts[2].to_string(),
                insertions: parts[0].parse().unwrap_or(0),
                deletions: parts[1].parse().unwrap_or(0),
            });
        }
    }

    Ok(Some(CommitInfo {
        id: commit_id,
        author: CommitAuthor {
            name: current_author_name,
            email: current_author_email,
        },
        date: commit_date,
        file_changes,
    }))
}

pub fn git_log_commits(
    since: &str,
    until: &str,
    cwd: &Path,
) -> Result<impl Iterator<Item = Result<CommitInfo, BoundError>>, BoundError> {
    let mut lines = git_log_lines(since, until, cwd)?.peekable();

    // Skip the first COMMIT line
    if let Some(line) = lines.next().transpose()? {
        if line != "COMMIT" {
            return Err(BoundError::UnexpectedCommitError(line));
        }
    }

    Ok(std::iter::from_fn(
        move || match next_commit_info_from_git_log_lines(&mut lines) {
            Ok(Some(commit_info)) => Some(Ok(commit_info)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        },
    ))
}

// CODEOWNERS

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

pub fn git_log_commits_with_codeowners<'a>(
    since: &str,
    until: &str,
    cwd: &'a Path,
    memberships: Option<&'a Vec<AuthorCodeownerMemberships>>,
) -> Result<impl Iterator<Item = Result<CommitInfoWithCodeowner, BoundError>> + 'a, BoundError> {
    let mut commits = git_log_commits(since, until, cwd)?;

    Ok(std::iter::from_fn(move || match commits.next() {
        Some(Ok(commit)) => {
            let codeowners_contents = get_codeowners_at_commit(&commit.id, cwd).ok().flatten();
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
                    let author_is_codeowner = memberships.as_ref().map(|m| {
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
    }))
}
