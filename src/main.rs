use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use git2::Repository;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(version = "1.0", author = "Your Name")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    #[clap(long, default_value = ".")]
    repo: PathBuf,
}
#[derive(Subcommand)]
enum Commands {
    FindFirstCommit {
        #[clap(help = "Date to search from (format: YYYY-MM-DD)")]
        date: String,
    },
    FindLastCommit {
        #[clap(help = "Date to search from (format: YYYY-MM-DD)")]
        date: String,
    },
    Commits {
        #[clap(long, short, help = "Start date (format: YYYY-MM-DD)")]
        since: String,
        #[clap(long, short, help = "End date (format: YYYY-MM-DD)")]
        until: String,
    },
}

fn handle_commits_command(
    repo: &Repository,
    since: &str,
    until: &str,
) -> Result<(), anyhow::Error> {
    let since_date = parse_date(since)?;
    let until_date = parse_date(until)?;

    let since_commit = bound::find_first_commit_on_or_after_date(repo, since_date)?
        .ok_or_else(|| anyhow::anyhow!("No commit found on or after {}", since_date))?;
    let until_commit = bound::find_last_commit_before_date(repo, until_date)?
        .ok_or_else(|| anyhow::anyhow!("No commit found before {}", until_date))?;

    for commit_result in bound::commits_between_asc(repo, &since_commit, &until_commit)? {
        let commit = commit_result?;
        let info = bound::get_commit_info(repo, &commit)?;

        println!(
            "Commit: {}\nAuthor: {}\nDate: {}\n",
            info.id,
            info.author,
            info.date.format("%Y-%m-%d %H:%M:%S")
        );

        println!("Files changed:");
        for file_change in &info.file_changes {
            println!(
                "  {}: +{} -{}",
                file_change.path, file_change.insertions, file_change.deletions
            );
        }

        println!(
            "Total: {} file(s) changed, {} insertion(s), {} deletion(s)\n",
            info.total_files_changed, info.total_insertions, info.total_deletions
        );
    }

    Ok(())
}

fn parse_date(date: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_str(&format!("{} 00:00:00 +0000", date), "%Y-%m-%d %H:%M:%S %z")
        .with_context(|| format!("Failed to parse date: {}", date))
        .map(|d| d.with_timezone(&Utc))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo = Repository::open(&cli.repo)
        .with_context(|| format!("Failed to open repository at {:?}", cli.repo))?;

    match &cli.command {
        Commands::FindFirstCommit { date } => {
            let date = parse_date(date)?;
            let commit = bound::find_first_commit_on_or_after_date(&repo, date)
                .with_context(|| format!("Failed to find commit on or after {}", date))?;
            match commit {
                Some(c) => println!("{}", c.id()),
                None => eprintln!("No commit found on or after {}", date),
            }
        }
        Commands::FindLastCommit { date } => {
            let date = parse_date(date)?;
            let commit = bound::find_last_commit_before_date(&repo, date)
                .with_context(|| format!("Failed to find commit on or before {}", date))?;
            match commit {
                Some(c) => println!("{}", c.id()),
                None => eprintln!("No commit found on or before {}", date),
            }
        }
        Commands::Commits { since, until } => {
            handle_commits_command(&repo, since, until)?;
        }
    }

    Ok(())
}
