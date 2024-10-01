use std::collections::HashMap;

use chrono::{DateTime, TimeZone, Utc};
use git2::{Commit, DiffOptions, Oid, Repository};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BoundError {
    #[error("Git error: {0}")]
    GitError(#[from] git2::Error),
    #[error("Invalid timestamp")]
    InvalidTimestamp,
}

/// Finds the first commit in the repository that was committed on or after the given date.
///
/// This function traverses the commit history of the repository, starting from the HEAD,
/// and returns the first commit it encounters that has a commit date on or after the
/// specified date.
///
/// # Arguments
///
/// * `repo` - A reference to the git2::Repository to search in
/// * `date` - The DateTime<Utc> to search from
///
/// # Returns
///
/// * `Ok(Some(Commit))` if a matching commit is found
/// * `Ok(None)` if no commit on or after the given date is found
/// * `Err(git2::Error)` if there's an error accessing the repository
pub fn find_first_commit_on_or_after_date(
    repo: &Repository,
    date: DateTime<Utc>,
) -> Result<Option<Commit>, git2::Error> {
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    walk.set_sorting(git2::Sort::TIME)?;

    let mut result = None;

    for oid in walk {
        let commit = repo.find_commit(oid?)?;
        let commit_time = commit.time();
        let commit_date =
            DateTime::<Utc>::from_timestamp(commit_time.seconds(), 0).expect("Invalid timestamp");

        if commit_date >= date {
            result = Some(commit);
        } else {
            break; // We've gone past the date we're looking for
        }
    }

    Ok(result)
}

/// Finds the last commit in the repository that was committed before the given date.
///
/// This function traverses the commit history of the repository, starting from the oldest commit,
/// and returns the newest commit it encounters that has a commit date strictly before the
/// specified date.
///
/// # Arguments
///
/// * `repo` - A reference to the git2::Repository to search in
/// * `date` - The DateTime<Utc> to search up to
///
/// # Returns
///
/// * `Ok(Some(Commit))` if a matching commit is found
/// * `Ok(None)` if no commit before the given date is found
/// * `Err(git2::Error)` if there's an error accessing the repository
pub fn find_last_commit_before_date(
    repo: &Repository,
    date: DateTime<Utc>,
) -> Result<Option<Commit>, git2::Error> {
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    walk.set_sorting(git2::Sort::TIME | git2::Sort::REVERSE)?;

    let mut last_commit = None;

    for oid in walk {
        let commit = repo.find_commit(oid?)?;
        let commit_time = commit.time();
        let commit_date =
            DateTime::<Utc>::from_timestamp(commit_time.seconds(), 0).expect("Invalid timestamp");

        if commit_date >= date {
            return Ok(last_commit);
        }

        last_commit = Some(commit);
    }

    Ok(last_commit)
}

pub fn commits_between_asc<'repo>(
    repo: &'repo Repository,
    start_commit: &Commit,
    end_commit: &Commit,
) -> Result<impl Iterator<Item = Result<Commit<'repo>, git2::Error>> + 'repo, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push(end_commit.id())?;
    revwalk.hide(start_commit.parent_id(0)?)?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;

    Ok(revwalk.map(move |oid_result| oid_result.and_then(|oid| repo.find_commit(oid))))
}

//////

pub struct CommitInfo {
    pub id: Oid,
    pub author: String,
    pub date: DateTime<Utc>,
    pub file_changes: Vec<FileChange>,
    pub total_files_changed: usize,
    pub total_insertions: usize,
    pub total_deletions: usize,
}

pub struct FileChange {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
}

pub fn get_commit_info(repo: &Repository, commit: &Commit) -> Result<CommitInfo, BoundError> {
    let (file_changes, total_stats) = get_commit_changes(repo, commit)?;

    Ok(CommitInfo {
        id: commit.id(),
        author: commit.author().name().unwrap_or("<unknown>").to_string(),
        date: Utc
            .timestamp_opt(commit.time().seconds(), 0)
            .single()
            .ok_or(BoundError::InvalidTimestamp)?,
        file_changes,
        total_files_changed: total_stats.files_changed(),
        total_insertions: total_stats.insertions(),
        total_deletions: total_stats.deletions(),
    })
}

fn get_commit_changes(
    repo: &Repository,
    commit: &Commit,
) -> Result<(Vec<FileChange>, git2::DiffStats), BoundError> {
    let parent = commit.parent(0).ok();
    let diff = if let Some(parent) = parent {
        repo.diff_tree_to_tree(
            Some(&parent.tree()?),
            Some(&commit.tree()?),
            Some(&mut DiffOptions::new()),
        )?
    } else {
        let empty_tree = repo.find_tree(repo.treebuilder(None)?.write()?)?;
        repo.diff_tree_to_tree(
            Some(&empty_tree),
            Some(&commit.tree()?),
            Some(&mut DiffOptions::new()),
        )?
    };

    let mut file_stats: HashMap<String, (usize, usize)> = HashMap::new();

    diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from("unknown"));

        let (insertions, deletions) = file_stats.entry(file_path).or_insert((0, 0));
        match line.origin() {
            '+' => *insertions += 1,
            '-' => *deletions += 1,
            _ => {}
        }
        true
    })?;

    let file_changes = file_stats
        .into_iter()
        .map(|(path, (insertions, deletions))| FileChange {
            path,
            insertions,
            deletions,
        })
        .collect();

    let total_stats = diff.stats()?;

    Ok((file_changes, total_stats))
}
