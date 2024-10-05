use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

use chrono::{DateTime, TimeZone, Utc};

use crate::BoundError;

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

pub struct GitLogLinesIterator {
    reader: BufReader<std::process::ChildStdout>,
}

impl Iterator for GitLogLinesIterator {
    type Item = Result<String, BoundError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(Ok(line.trim_end().to_string())),
            Err(e) => Some(Err(BoundError::GitExecutionError(e))),
        }
    }
}

fn git_log_lines(since: &str, until: &str, cwd: &Path) -> Result<GitLogLinesIterator, BoundError> {
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

    Ok(GitLogLinesIterator {
        reader: BufReader::new(output),
    })
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

struct GitLogIterator<I> {
    lines: I,
}

impl<I> Iterator for GitLogIterator<I>
where
    I: Iterator<Item = Result<String, BoundError>>,
{
    type Item = Result<CommitInfo, BoundError>;

    fn next(&mut self) -> Option<Self::Item> {
        match next_commit_info_from_git_log_lines(&mut self.lines) {
            Ok(Some(commit_info)) => Some(Ok(commit_info)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

pub fn git_log_commits(
    since: &str,
    until: &str,
    cwd: &Path,
) -> Result<impl Iterator<Item = Result<CommitInfo, BoundError>>, BoundError> {
    let mut lines = git_log_lines(since, until, cwd)?;

    // Skip the first COMMIT line
    if let Some(line) = lines.next().transpose()? {
        if line != "COMMIT" {
            return Err(BoundError::UnexpectedCommitError(line));
        }
    }

    Ok(GitLogIterator { lines })
}
