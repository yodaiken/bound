use chrono::{Duration, Utc};

use chrono::{FixedOffset, TimeZone};
use git2::{Commit, Repository};
use std::collections::HashMap;
use std::io::Cursor;

use thiserror::Error;

use clap::Parser;

#[derive(Error, Debug)]
enum BoundError {
    #[error("Not a valid git repository: {0}")]
    InvalidRepository(String),
    #[error(transparent)]
    Git(#[from] git2::Error),
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = ".")]
    repo: String,
    #[arg(short, long)]
    since: Option<u32>,
}

fn main() -> Result<(), BoundError> {
    let args = Args::parse();
    let repo = Repository::open(&args.repo)
        .map_err(|_| BoundError::InvalidRepository(args.repo.clone()))?;
    let mut revwalk = repo.revwalk()?;

    revwalk.push_head()?;

    revwalk.set_sorting(git2::Sort::TIME)?;

    let mut author_file_stats: HashMap<(String, String, Vec<String>), (usize, usize, usize)> =
        HashMap::new();

    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let author = commit.author().name().unwrap_or("Unknown").to_string();

        let tree = commit.tree()?;
        let potential_codeowner_paths = vec![".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];
        let codeowners_contents = potential_codeowner_paths.iter().find_map(|path| {
            if let Ok(entry) = tree.get_path(std::path::Path::new(path)) {
                let object = entry.to_object(&repo).ok()?;
                let blob = object.as_blob()?;
                let content = std::str::from_utf8(blob.content()).ok()?;
                Some(content.to_string())
            } else {
                None
            }
        });
        let codeowners = if let Some(contents) = codeowners_contents {
            codeowners::from_reader(Cursor::new(contents))
        } else {
            println!(
                "Warning: No CODEOWNERS file found in commit {}",
                commit.id()
            );
            codeowners::from_reader(Cursor::new("".as_bytes()))
        };

        let file_changes = get_commit_changes(&repo, &commit)?;
        for (file, changes) in file_changes {
            let owners = codeowners
                .of(&file)
                .unwrap_or(&Vec::new())
                .iter()
                .map(|owner| owner.to_string())
                .collect::<Vec<String>>();
            let stats = author_file_stats
                .entry((author.clone(), file, owners))
                .or_insert((0, 0, 0));
            stats.0 += 1; // Increment commit count
            stats.1 += changes.additions;
            stats.2 += changes.deletions;
        }

        // Break if since is set and we are past the time
        if let Some(since) = args.since {
            let commit_time = commit.time();
            let commit_date = FixedOffset::east_opt(commit_time.offset_minutes() * 60)
                .unwrap()
                .timestamp_opt(commit_time.seconds(), 0)
                .unwrap();
            let now = Utc::now();
            let since = now - Duration::days(since as i64);
            if commit_date < since {
                break;
            }
        }
    }

    print_author_file_statistics(&author_file_stats);

    Ok(())
}

fn print_author_file_statistics(
    stats: &HashMap<(String, String, Vec<String>), (usize, usize, usize)>,
) {
    for ((author, file, owners), (commits, additions, deletions)) in stats {
        println!("{}: {} (Owners: {})", author, file, owners.join(", "));
        println!("  C {}  +{}  -{}", commits, additions, deletions);
    }
}

fn get_commit_changes(
    repo: &Repository,
    commit: &Commit,
) -> Result<HashMap<String, FileChanges>, git2::Error> {
    let parent = commit.parent(0).ok();
    let a = parent.as_ref().map(|c| c.tree()).transpose()?;
    let b = commit.tree()?;

    let diff = repo.diff_tree_to_tree(a.as_ref(), Some(&b), None)?;
    let mut file_stats = HashMap::new();

    diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from(""));

        let stats = file_stats
            .entry(file_path)
            .or_insert(FileChanges::default());
        match line.origin() {
            '+' => stats.additions += 1,
            '-' => stats.deletions += 1,
            _ => {}
        }
        true
    })?;

    Ok(file_stats)
}

#[derive(Default)]
struct FileChanges {
    additions: usize,
    deletions: usize,
}
