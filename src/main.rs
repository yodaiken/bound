use anyhow::Result;

use bound::{git_log_commits, AuthorCodeownerMemberships};
use clap::{Parser, Subcommand};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

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
        #[arg(short, long)]
        memberships: Option<PathBuf>,
    },
}

fn parse_memberships(path: &PathBuf) -> Result<Vec<AuthorCodeownerMemberships<'static>>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut memberships = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid format in memberships file"));
        }
        memberships.push(AuthorCodeownerMemberships {
            author_email: if parts[0].is_empty() {
                None
            } else {
                Some(Box::leak(parts[0].to_string().into_boxed_str()))
            },
            author_name: if parts[1].is_empty() {
                None
            } else {
                Some(Box::leak(parts[1].to_string().into_boxed_str()))
            },
            codeowner: Box::leak(parts[2].to_string().into_boxed_str()),
        });
    }

    Ok(memberships)
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
            memberships: memberships_path,
        } => {
            let memberships = memberships_path
                .as_ref()
                .map(parse_memberships)
                .transpose()?;

            let commits = bound::git_log_commits_with_codeowners(
                since,
                until,
                directory,
                memberships.as_ref(),
            )?;
            for commit in commits {
                let commit = commit?;
                println!("Commit: {}", commit.id);
                println!("Author: {} <{}>", commit.author.name, commit.author.email);
                println!("Date: {}", commit.date);
                println!("Changes:");
                for change in commit.file_changes {
                    println!(
                        "  {}: +{} -{} (Codeowners: {} {})",
                        change.path,
                        change.insertions,
                        change.deletions,
                        change
                            .author_is_codeowner
                            .map_or("-", |b| if b { "Y" } else { "N" }),
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
