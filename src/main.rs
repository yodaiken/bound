use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use git2::Repository;
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Debug)]
enum StatsSplit {
    Weekly,
    Daily,
    Monthly,
    Never,
}

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
    FileStats {
        #[clap(long, short, help = "Start date (format: YYYY-MM-DD)")]
        since: String,
        #[clap(long, short, help = "End date (format: YYYY-MM-DD)")]
        until: String,
        #[clap(
            long,
            value_enum,
            help = "Split option (weekly, daily, monthly, or never)",
            default_value_t = StatsSplit::Never
        )]
        split: StatsSplit,
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

fn handle_file_stats_command(
    repo: &Repository,
    since: &str,
    until: &str,
    split: StatsSplit,
) -> Result<(), anyhow::Error> {
    let since_date = parse_date(since)?;
    let until_date = parse_date(until)?;

    let since_commit = bound::find_first_commit_on_or_after_date(repo, since_date)?
        .ok_or_else(|| anyhow::anyhow!("No commit found on or after {}", since_date))?;
    let until_commit = bound::find_last_commit_before_date(repo, until_date)?
        .ok_or_else(|| anyhow::anyhow!("No commit found before {}", until_date))?;

    let commits = bound::commits_between_asc(repo, &since_commit, &until_commit)?;

    let date_group_fn = match split {
        StatsSplit::Weekly => |date: DateTime<Utc>| date.format("%Y-W%W").to_string(),
        StatsSplit::Daily => |date: DateTime<Utc>| date.format("%Y-%m-%d").to_string(),
        StatsSplit::Monthly => |date: DateTime<Utc>| date.format("%Y-%m").to_string(),
        StatsSplit::Never => |_: DateTime<Utc>| String::from("All"),
    };

    let file_stats = bound::collect_file_stats(repo, commits, date_group_fn)?;

    println!("DateGroup\tCodeowner\tAuthor\tFile\tInsertions\tDeletions");
    for stat in file_stats {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            stat.date_group.replace("\t", "\\t"),
            stat.codeowner.replace("\t", "\\t"),
            stat.author.replace("\t", "\\t"),
            stat.path.replace("\t", "\\t"),
            stat.insertions,
            stat.deletions
        );
    }

    Ok(())
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
        Commands::FileStats {
            since,
            until,
            split,
        } => {
            handle_file_stats_command(&repo, since, until, split.clone())?;
        }
    }

    Ok(())
}
