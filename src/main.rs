use anyhow::Result;

use bound::{
    get_github_team_members, get_github_team_slugs, get_user_info, git_log_commits,
    read_memberships_from_tsv, AuthorCodeownerMemberships,
};
use clap::{Parser, Subcommand};
use std::{collections::HashMap, path::PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

async fn get_all_org_members(
    api: &GithubApi,
    org: &str,
) -> Result<Vec<AuthorCodeownerMemberships>> {
    let teams = get_github_team_slugs(api, org).await?;

    let mut all_members = Vec::new();
    let mut team_members = HashMap::new();
    let progress = ProgressBar::new(teams.len() as u64);
    let pb_style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} teams")
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    progress.set_style(pb_style);
    for team in teams {
        let members = get_github_team_members(api, org, &team).await?;
        all_members.extend(members.iter().cloned());
        team_members.insert(team, members);
        progress.inc(1);
    }
    progress.finish_with_message("All teams processed");

    let total_members = all_members.len();
    let member_progress = ProgressBar::new(total_members as u64);
    let member_style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.green/white} {pos}/{len} members")
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    member_progress.set_style(member_style);

    let mut acms = Vec::new();
    for (team, members) in team_members {
        for member in members {
            if let Some((name, email)) = get_user_info(api, &member).await? {
                acms.push(AuthorCodeownerMemberships {
                    author_email: Some(email),
                    author_name: Some(name),
                    codeowner: format!("@{}/{}", org, team),
                });
            }
            member_progress.inc(1);
        }
    }

    member_progress.finish_with_message("All members processed");

    Ok(acms)
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    GhGetOrgLogins,
    GhGetToken,

    GhGetTeamSlugs {
        org: String,
    },

    GhGetTeamMembers {
        #[arg(short, long)]
        org: String,
        #[arg(short, long)]
        team: String,
    },

    GhGetUserNameEmail {
        logins: Vec<String>,
    },

    GhGenerateOwnersFile {
        org: String,

        #[arg(short, long, default_value = "codeowners.tsv")]
        output: PathBuf,
    },

    PrintCommits {
        #[arg(short, long)]
        since: String,
        #[arg(short, long)]
        until: String,
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        #[arg(long)]
        tsv: bool,
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
        #[arg(long)]
        tsv: bool,
    },
}

use bound::GithubApi;
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::GhGetOrgLogins => {
            let api = GithubApi::new()?;
            let orgs = bound::get_github_org_logins(&api).await?;
            for org in orgs {
                println!("{}", org);
            }
        }
        Commands::GhGetToken => {
            let token = bound::get_token()?;
            println!("Token: {}", token);
        }

        Commands::GhGetTeamSlugs { org } => {
            let api = GithubApi::new()?;
            let slugs = bound::get_github_team_slugs(&api, org).await?;
            for slug in slugs {
                println!("{}", slug);
            }
        }

        Commands::GhGetTeamMembers { org, team } => {
            let api = GithubApi::new()?;
            let members = bound::get_github_team_members(&api, org, team).await?;
            for member in members {
                println!("{}", member);
            }
        }

        Commands::GhGenerateOwnersFile { org, output } => {
            let api = GithubApi::new()?;
            let memberships = get_all_org_members(&api, org).await?;
            bound::write_memberships_to_tsv(&memberships, output)?;
        }

        Commands::GhGetUserNameEmail { logins } => {
            let api = GithubApi::new()?;
            for login in logins {
                match bound::get_user_info(&api, &login).await? {
                    Some((name, email)) => {
                        if email.is_empty() {
                            println!("{} <not found>", name);
                        } else {
                            println!("{} <{}>", name, email);
                        }
                    }
                    None => println!("{} <not found>", login),
                }
            }
        }

        Commands::PrintCommits {
            since,
            until,
            directory,
            tsv,
        } => {
            let commits = git_log_commits(since, until, directory)?;
            if *tsv {
                println!("commit_id\tauthor_name\tauthor_email\tdate\tpath\tinsertions\tdeletions");
                for commit in commits {
                    let commit = commit?;
                    for change in commit.file_changes {
                        println!(
                            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                            commit.id,
                            commit.author_name,
                            commit.author_email,
                            commit.timestamp,
                            change.path,
                            change.insertions,
                            change.deletions
                        );
                    }
                }
            } else {
                for commit in commits {
                    let commit = commit?;
                    println!("Commit: {}", commit.id);
                    println!("Author: {} <{}>", commit.author_name, commit.author_email);
                    println!("Date: {}", commit.timestamp);
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
            tsv,
        } => {
            let memberships = memberships_path
                .as_ref()
                .map(read_memberships_from_tsv)
                .transpose()?;

            let commits =
                bound::git_log_commits_with_codeowners(since, until, directory, memberships)?;

            if *tsv {
                println!("commit_id\tauthor_name\tauthor_email\tdate\tpath\tinsertions\tdeletions\tauthor_is_codeowner\tcodeowners");
                for commit in commits {
                    let commit = commit?;
                    for change in commit.file_changes {
                        println!(
                            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                            commit.id,
                            commit.author_name,
                            commit.author_email,
                            commit.timestamp,
                            change.path,
                            change.insertions,
                            change.deletions,
                            change.author_is_codeowner.map_or("", |b| if b {
                                "true"
                            } else {
                                "false"
                            }),
                            change
                                .codeowners
                                .as_ref()
                                .map_or_else(|| "".to_string(), |owners| owners.join(", "))
                        );
                    }
                }
            } else {
                for commit in commits {
                    let commit = commit?;
                    println!("Commit: {}", commit.id);
                    println!("Author: {} <{}>", commit.author_name, commit.author_email);
                    println!("Date: {}", commit.timestamp);
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
    }

    Ok(())
}
