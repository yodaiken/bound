use anyhow::Result;

use bound::{
    get_github_team_members, get_github_team_slugs, get_user_info, git_log_commits,
    read_memberships_from_tsv, AuthorCodeownerMemberships,
};
use clap::{Parser, Subcommand};
use std::{collections::HashMap, io::Write, path::PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

use std::collections::HashSet;

pub fn create_author_codeowner_map(
    memberships: Vec<AuthorCodeownerMemberships>,
) -> HashMap<(String, String), HashSet<String>> {
    let mut map = HashMap::new();

    for membership in memberships {
        let key = (
            membership.author_name.unwrap_or_default(),
            membership.author_email.unwrap_or_default(),
        );
        map.entry(key)
            .or_insert_with(HashSet::new)
            .insert(membership.codeowner);
    }

    map
}

fn create_teams(
    memberships: Vec<AuthorCodeownerMemberships>,
) -> Result<HashMap<(String, String), String>> {
    let m = create_author_codeowner_map(memberships);

    let mut res = HashMap::new();

    for (key, value) in m {
        let team = if value.len() == 1 {
            value.iter().next().unwrap().clone()
        } else {
            println!("What team does {} <>{} belong to?", key.0, key.1);
            for (index, codeowner) in value.iter().enumerate() {
                println!("{}. {}", index + 1, codeowner);
            }
            print!("Enter your choice (1-{}): ", value.len());
            std::io::stdout().flush()?;
            let mut choice = String::new();
            std::io::stdin().read_line(&mut choice)?;
            let choice: usize = choice.trim().parse()?;
            if choice > 0 && choice <= value.len() {
                value.iter().nth(choice - 1).unwrap().clone()
            } else {
                return Err(anyhow::anyhow!("Invalid choice"));
            }
        };

        res.insert(key, team);
    }

    return Ok(res);
}

fn write_teams_to_tsv(teams: &HashMap<(String, String), String>, output: &PathBuf) -> Result<()> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(output)?;

    writer.write_record(&["author_name", "author_email", "team"])?;

    for ((name, email), team) in teams {
        writer.write_record(&[name, email, team])?;
    }

    writer.flush()?;
    Ok(())
}

async fn get_all_org_members(
    api: &GithubApi,
    org: &str,
) -> Result<Vec<AuthorCodeownerMemberships>> {
    let teams = get_github_team_slugs(api, org).await?;

    let mut all_members = HashSet::new();
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

    let mut user_cache: HashMap<String, (String, String)> = HashMap::new();
    let mut acms = Vec::new();
    for (team, members) in team_members {
        for member in members {
            let (name, email) = if let Some(info) = user_cache.get(&member) {
                info.clone()
            } else {
                if let Some(info) = get_user_info(api, &member).await? {
                    user_cache.insert(member.clone(), info.clone());
                    info
                } else {
                    member_progress.inc(1);
                    continue;
                }
            };
            acms.push(AuthorCodeownerMemberships {
                author_email: Some(email),
                author_name: Some(name),
                codeowner: format!("@{}/{}", org, team),
            });
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
    CreateTeams {
        #[arg(short, long, default_value = "codeowners.tsv")]
        input: PathBuf,
        #[arg(short, long, default_value = "teams.tsv")]
        output: PathBuf,
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

        Commands::CreateTeams { input, output } => {
            let memberships = read_memberships_from_tsv(&input)?;
            let teams = create_teams(memberships)?;
            write_teams_to_tsv(&teams, &output)?;
            println!("Teams created and written to {}", output.display());
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
