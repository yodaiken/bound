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

// ----

pub struct ContributorFileStats<T> {
    pub author: String,
    pub codeowner: String,
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
    pub date_group: T,
}

pub fn collect_file_stats<'repo, T: Clone, F>(
    repo: &'repo Repository,
    commits: impl Iterator<Item = Result<Commit<'repo>, git2::Error>>,
    date_group_fn: F,
) -> Result<Vec<ContributorFileStats<T>>, BoundError>
where
    F: Fn(DateTime<Utc>) -> T,
    T: Ord + Clone,
{
    let mut file_stats: Vec<ContributorFileStats<T>> = Vec::new();

    for commit_result in commits {
        let commit = commit_result?;
        let commit_info = get_commit_info(repo, &commit)?;
        let date_group = date_group_fn(commit_info.date);

        let tree = commit.tree()?;
        let codeowners = get_codeowners(repo, &tree);

        for file_change in commit_info.file_changes {
            let codeowner = get_codeowner(&codeowners, &file_change.path);

            let index = file_stats
                .binary_search_by(|stats| {
                    stats
                        .date_group
                        .cmp(&date_group)
                        .then(stats.codeowner.cmp(&codeowner))
                        .then(stats.author.cmp(&commit_info.author))
                        .then(stats.path.cmp(&file_change.path))
                })
                .unwrap_or_else(|x| x);

            if index < file_stats.len()
                && file_stats[index].date_group == date_group
                && file_stats[index].codeowner == codeowner
                && file_stats[index].author == commit_info.author
                && file_stats[index].path == file_change.path
            {
                file_stats[index].insertions += file_change.insertions;
                file_stats[index].deletions += file_change.deletions;
            } else {
                file_stats.insert(
                    index,
                    ContributorFileStats {
                        author: commit_info.author.clone(),
                        path: file_change.path,
                        codeowner,
                        insertions: file_change.insertions,
                        deletions: file_change.deletions,
                        date_group: date_group.clone(),
                    },
                );
            }
        }
    }

    Ok(file_stats)
}

fn get_codeowner(codeowners: &codeowners::Owners, path: &str) -> String {
    match codeowners.of(path) {
        None => "<Unowned>".to_owned(),
        Some(owners) => owners
            .iter()
            .map(|owner| owner.to_string())
            .collect::<Vec<String>>()
            .join(", "),
    }
}

fn get_codeowners(repo: &Repository, tree: &git2::Tree) -> codeowners::Owners {
    let potential_codeowner_paths = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];
    let codeowners_contents = potential_codeowner_paths.iter().find_map(|path| {
        tree.get_path(std::path::Path::new(path))
            .ok()
            .and_then(|entry| entry.to_object(repo).ok())
            .and_then(|object| object.into_blob().ok())
    });

    if let Some(blob) = codeowners_contents {
        codeowners::from_reader(blob.content())
    } else {
        // prinwarn!("Warning: No CODEOWNERS file found in this commit");
        codeowners::from_reader(&[] as &[u8])
    }
}
