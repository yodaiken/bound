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
enum Command {
    #[command(name = "codeowner-analyze")]
    CodeownerAnalyze,
    #[command(name = "contributor-analyze")]
    ContributorAnalyze,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,

    #[arg(short, long, default_value = ".")]
    repo: String,

    #[arg(short, long)]
    since: Option<u32>,

    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<(), BoundError> {
    let args = Args::parse();
    let repo = Repository::open(&args.repo)
        .map_err(|_| BoundError::InvalidRepository(args.repo.clone()))?;

    let analysis_data = analyze_repository(&repo, args.since)?;

    match args.command {
        Command::CodeownerAnalyze => print_codeowner_analysis(&analysis_data, args.verbose),
        Command::ContributorAnalyze => print_contributor_analysis(&analysis_data, args.verbose),
    }

    Ok(())
}

fn print_codeowner_analysis(analysis_data: &AnalysisData, verbose: bool) {
    println!("Codeowner\tContributor\tCommits\tAdditions\tDeletions\tTotal Changes");
    for (owner, contributors) in &analysis_data.codeowner_stats {
        for (contributor, (commits, additions, deletions)) in contributors {
            let total_changes = additions + deletions;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                owner, contributor, commits, additions, deletions, total_changes
            );
        }
        if verbose {
            if let Some(files) = analysis_data.file_details.get(owner) {
                println!("\n\nCodeowner\tFile\tCommits\tAdditions\tDeletions\tTotal Changes");
                for (file, (commits, additions, deletions)) in files {
                    let total_changes = additions + deletions;
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        owner, file, commits, additions, deletions, total_changes
                    );
                }
            }
        }
    }
}

fn print_contributor_analysis(analysis_data: &AnalysisData, verbose: bool) {
    println!("Contributor\tCodeowner\tCommits\tAdditions\tDeletions\tTotal Changes");
    for (contributor, owners) in &analysis_data.contributor_stats {
        for (owner, (commits, additions, deletions)) in owners {
            let total_changes = additions + deletions;
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                contributor, owner, commits, additions, deletions, total_changes
            );
        }
        if verbose {
            println!(
                "\n\nContributor\tCodeowner\tFile\tCommits\tAdditions\tDeletions\tTotal Changes"
            );
            for (owner, files) in &analysis_data.file_details {
                if let Some(owner_stats) = analysis_data.contributor_stats.get(contributor) {
                    if owner_stats.contains_key(owner) {
                        for (file, (file_commits, file_additions, file_deletions)) in files {
                            let total_changes = file_additions + file_deletions;
                            println!(
                                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                                contributor,
                                owner,
                                file,
                                file_commits,
                                file_additions,
                                file_deletions,
                                total_changes
                            );
                        }
                    }
                }
            }
        }
    }
}

fn analyze_repository(repo: &Repository, since: Option<u32>) -> Result<AnalysisData, BoundError> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    let mut analysis_data = AnalysisData::default();

    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let author = commit.author().name().unwrap_or("Unknown").to_string();

        let tree = commit.tree()?;
        let codeowners = get_codeowners(repo, &tree);

        let file_changes = get_commit_changes(repo, &commit)?;
        for (file, changes) in file_changes {
            let owners = codeowners
                .of(&file)
                .map(|owners| {
                    owners
                        .iter()
                        .map(|owner| owner.to_string())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();
            update_stats(&mut analysis_data, &author, &file, &owners, &changes);
        }

        if should_break(since, &commit) {
            break;
        }
    }

    Ok(analysis_data)
}

fn get_codeowners(repo: &Repository, tree: &git2::Tree) -> codeowners::Owners {
    let potential_codeowner_paths = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];
    let codeowners_contents = potential_codeowner_paths.iter().find_map(|path| {
        if let Ok(entry) = tree.get_path(std::path::Path::new(path)) {
            let object = entry.to_object(repo).ok()?;
            let blob = object.as_blob()?;
            let content = std::str::from_utf8(blob.content()).ok()?;
            Some(content.to_string())
        } else {
            None
        }
    });

    if let Some(contents) = codeowners_contents {
        codeowners::from_reader(Cursor::new(contents))
    } else {
        // prinwarn!("Warning: No CODEOWNERS file found in this commit");
        codeowners::from_reader(Cursor::new("".as_bytes()))
    }
}

fn update_stats(
    analysis_data: &mut AnalysisData,
    author: &str,
    file: &str,
    owners: &[String],
    changes: &FileChanges,
) {
    let effective_owners = if owners.is_empty() {
        vec![String::from("<UNOWNED>")]
    } else {
        owners.to_vec()
    };

    for owner in &effective_owners {
        let codeowner_stats = analysis_data
            .codeowner_stats
            .entry(owner.to_string())
            .or_default()
            .entry(author.to_string())
            .or_insert((0, 0, 0));
        codeowner_stats.0 += 1;
        codeowner_stats.1 += changes.additions;
        codeowner_stats.2 += changes.deletions;

        let contributor_stats = analysis_data
            .contributor_stats
            .entry(author.to_string())
            .or_default()
            .entry(owner.to_string())
            .or_insert((0, 0, 0));
        contributor_stats.0 += 1;
        contributor_stats.1 += changes.additions;
        contributor_stats.2 += changes.deletions;

        let file_details = analysis_data
            .file_details
            .entry(owner.to_string())
            .or_default()
            .entry(file.to_string())
            .or_insert((0, 0, 0));
        file_details.0 += 1;
        file_details.1 += changes.additions;
        file_details.2 += changes.deletions;
    }
}

fn should_break(since: Option<u32>, commit: &git2::Commit) -> bool {
    if let Some(since) = since {
        let commit_time = commit.time();
        let commit_date = FixedOffset::east_opt(commit_time.offset_minutes() * 60)
            .unwrap()
            .timestamp_opt(commit_time.seconds(), 0)
            .unwrap();
        let now = Utc::now();
        let since = now - Duration::days(since as i64);
        commit_date < since
    } else {
        false
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

#[derive(Default)]
struct AnalysisData {
    codeowner_stats: HashMap<String, HashMap<String, (usize, usize, usize)>>,
    contributor_stats: HashMap<String, HashMap<String, (usize, usize, usize)>>,
    file_details: HashMap<String, HashMap<String, (usize, usize, usize)>>,
}
