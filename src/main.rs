use anyhow::Result;

use bound::git_log_commits;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    PrintCommits {
        #[arg(short, long)]
        since: String,
        #[arg(short, long)]
        until: String,
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
    },
    GetCodeowners {
        #[arg(short, long)]
        commit: String,
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
    },
    PrintCommitsWithCodeowners {
        #[arg(short, long)]
        since: String,
        #[arg(short, long)]
        until: String,
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
    },
}
fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::PrintCommits {
            since,
            until,
            directory,
        } => {
            let commits = git_log_commits(since, until, directory)?;
            for commit in commits {
                let commit = commit?;
                println!("Commit: {}", commit.id);
                println!("Author: {} <{}>", commit.author.name, commit.author.email);
                println!("Date: {}", commit.date);
                println!("Changes:");
                for change in commit.file_changes {
                    println!(
                        "  {}: +{} -{}",
                        change.path, change.insertions, change.deletions
                    );
                }
                println!();
            }
        }
        Commands::GetCodeowners { commit, directory } => {
            let codeowners = bound::get_codeowners_at_commit(commit, directory)?;
            match codeowners {
                Some(content) => println!("{}", content),
                None => eprintln!("No CODEOWNERS file found at this commit."),
            }
        }
        Commands::PrintCommitsWithCodeowners {
            since,
            until,
            directory,
        } => {
            let commits = bound::git_log_commits_with_codeowners(since, until, directory)?;
            for commit in commits {
                let commit = commit?;
                println!("Commit: {}", commit.id);
                println!("Author: {} <{}>", commit.author.name, commit.author.email);
                println!("Date: {}", commit.date);
                println!("Changes:");
                for change in commit.file_changes {
                    println!(
                        "  {}: +{} -{} (Codeowners: {})",
                        change.path,
                        change.insertions,
                        change.deletions,
                        change
                            .codeowners
                            .as_ref()
                            .map_or_else(|| "None".to_string(), |owners| owners.join(", "))
                    );
                }
                println!();
            }
        }
    }

    Ok(())
}
