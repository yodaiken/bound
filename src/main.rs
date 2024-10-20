use anyhow::Result;

use bound::{
    get_github_team_members, get_github_team_slugs, get_user_info, git_log_commits,
    read_memberships_from_tsv, AuthorCodeownerMemberships,
};
use clap::{Parser, Subcommand};
use std::{collections::HashMap, path::PathBuf};

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

async fn get_all_org_members(
    api: &GithubApi,
    org: &str,
) -> Result<Vec<AuthorCodeownerMemberships>> {
    let progress_style = ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap();
    let progress = ProgressBar::new_spinner();
    progress.set_style(progress_style);
    progress.set_message("Fetching GitHub team slugs...");

    let teams = get_github_team_slugs(api, org).await?;

    progress.finish_with_message("GitHub team slugs fetched successfully.");

    let num_teams = teams.len();

    let progress_style = ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap();
    let progress = ProgressBar::new_spinner();
    progress.set_style(progress_style);
    progress.set_message("Fetching all codeowners...");

    let all_codeowners = bound::get_all_codeowners(&std::path::PathBuf::from("."))?;

    progress.finish_with_message("All codeowners fetched successfully.");

    // Filter teams to only include those that are codeowners
    let teams: Vec<String> = teams
        .into_iter()
        .filter(|team| all_codeowners.contains(&format!("@{}/{}", org, team)))
        .collect();

    println!(
        "Fetched {} Github Teams in {}, eliminated {} non-codeowning teams.",
        num_teams,
        org,
        num_teams - teams.len(),
    );

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
            } else if let Some(info) = get_user_info(api, &member).await? {
                user_cache.insert(member.clone(), info.clone());
                info
            } else {
                member_progress.inc(1);
                continue;
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
enum DevCommands {
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
    GhGetOrgLogins,
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
    GetAllCodeowners {
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
        #[arg(short, long, default_value = "codeowners.tsv")]
        codeowners_path: Option<PathBuf>,
        #[arg(long)]
        tsv: bool,
    },
}
#[derive(Subcommand)]
enum Commands {
    #[command(subcommand)]
    Dev(DevCommands),
    Init {
        org: String,

        #[arg(short, long, default_value = "codeowners.tsv")]
        codeowners_path: PathBuf,
    },
    AnalyzeByOwner {
        #[arg(short, long)]
        since: String,
        #[arg(short, long)]
        until: String,
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        #[arg(short, long, default_value = "codeowners.tsv")]
        codeowners_path: PathBuf,
        #[arg(long)]
        adjusted: bool,
    },
    AnalyzeByContributor {
        #[arg(short, long)]
        since: String,
        #[arg(short, long)]
        until: String,
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        #[arg(short, long, default_value = "codeowners.tsv")]
        codeowners_path: PathBuf,
        #[arg(short, long)]
        owner: Option<String>,
        #[arg(long)]
        tsv: bool,
        #[arg(long)]
        adjusted: bool,
    },
}

use bound::GithubApi;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Dev(dev_command) => match dev_command {
            DevCommands::GhGetToken => {
                let token = bound::get_token()?;
                println!("Token: {}", token);
            }
            DevCommands::GhGetTeamSlugs { org } => {
                let api = GithubApi::new()?;
                let slugs = bound::get_github_team_slugs(&api, org).await?;
                for slug in slugs {
                    println!("{}", slug);
                }
            }
            DevCommands::GhGetTeamMembers { org, team } => {
                let api = GithubApi::new()?;
                let members = bound::get_github_team_members(&api, org, team).await?;
                for member in members {
                    println!("{}", member);
                }
            }
            DevCommands::GhGetUserNameEmail { logins } => {
                let api = GithubApi::new()?;
                for login in logins {
                    match bound::get_user_info(&api, login).await? {
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
            DevCommands::GhGetOrgLogins => {
                let api = GithubApi::new()?;
                let orgs = bound::get_github_org_logins(&api).await?;
                for org in orgs {
                    println!("{}", org);
                }
            }
            DevCommands::PrintCommits {
                since,
                until,
                directory,
                tsv,
            } => {
                let commits = git_log_commits(since, until, directory)?;
                if *tsv {
                    println!(
                        "commit_id\tauthor_name\tauthor_email\tdate\tpath\tinsertions\tdeletions"
                    );
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
            DevCommands::GetCodeowners { commit, directory } => {
                let codeowners = bound::get_codeowners_at_commit(commit, directory)?;
                match codeowners {
                    Some(content) => println!("{}", content),
                    None => eprintln!("No CODEOWNERS file found at this commit."),
                }
            }
            DevCommands::GetAllCodeowners { directory } => {
                let codeowners = bound::get_all_codeowners(directory)?;
                for codeowner in codeowners {
                    println!("{}", codeowner);
                }
            }

            DevCommands::PrintCommitsWithCodeowners {
                since,
                until,
                directory,
                codeowners_path: memberships_path,
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
                                change.author_is_codeowner.map_or("-", |b| if b {
                                    "Y"
                                } else {
                                    "N"
                                }),
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
        },
        Commands::Init {
            org,
            codeowners_path,
        } => {
            let api = GithubApi::new()?;
            let memberships = get_all_org_members(&api, org).await?;
            bound::write_memberships_to_tsv(&memberships, codeowners_path)?;
        }
        Commands::AnalyzeByOwner {
            since,
            until,
            directory,
            codeowners_path,
            adjusted,
        } => {
            let memberships = read_memberships_from_tsv(codeowners_path)?;
            let commits =
                bound::git_log_commits_with_codeowners(since, until, directory, Some(memberships))?;
            let analysis = bound::analyze_by_owner(commits, *adjusted)?;
            for owner_info in analysis {
                println!("Owner: {}", owner_info.owner);
                println!(
                    "  Team Changes: {} (+{}, -{})",
                    owner_info.total_insertions_by_team + owner_info.total_deletions_by_team,
                    owner_info.total_insertions_by_team,
                    owner_info.total_deletions_by_team
                );
                println!("  Team Commits: {:.2}", owner_info.total_commits_by_team);
                if *adjusted {
                    println!(
                        "  Adjusted Team Changes: {} (Commits: {:.2})",
                        owner_info.adjusted_changes_by_team, owner_info.adjusted_commits_by_team
                    );
                }
                println!(
                    "  Others Changes: {} (+{}, -{})",
                    owner_info.total_insertions_by_others + owner_info.total_deletions_by_others,
                    owner_info.total_insertions_by_others,
                    owner_info.total_deletions_by_others
                );
                println!(
                    "  Others Commits: {:.2}",
                    owner_info.total_commits_by_others
                );
                if *adjusted {
                    println!(
                        "  Adjusted Others Changes: {} (Commits: {:.2})",
                        owner_info.adjusted_changes_by_others,
                        owner_info.adjusted_commits_by_others
                    );
                }
                println!("  Top Outside Contributors by Changes:");
                for contributor in &owner_info.top_outside_contributors_by_changes {
                    println!(
                        "    {} <{}>: {}",
                        contributor.author_name, contributor.author_email, contributor.metric_value
                    );
                }
                println!("  Top Outside Contributors by Commits:");
                for contributor in &owner_info.top_outside_contributors_by_commits {
                    println!(
                        "    {} <{}>: {}",
                        contributor.author_name, contributor.author_email, contributor.metric_value
                    );
                }
                println!("  Top Team Contributors by Changes:");
                for contributor in &owner_info.top_team_contributors_by_changes {
                    println!(
                        "    {} <{}>: {}",
                        contributor.author_name, contributor.author_email, contributor.metric_value
                    );
                }
                println!("  Top Team Contributors by Commits:");
                for contributor in &owner_info.top_team_contributors_by_commits {
                    println!(
                        "    {} <{}>: {}",
                        contributor.author_name, contributor.author_email, contributor.metric_value
                    );
                }
                println!();
            }
        }
        Commands::AnalyzeByContributor {
            since,
            until,
            directory,
            codeowners_path,
            owner,
            tsv,
            adjusted,
        } => {
            let memberships = read_memberships_from_tsv(codeowners_path)?;

            let filter_authors = if let Some(owner) = owner {
                Some(
                    memberships
                        .iter()
                        .filter(|m| &m.codeowner == owner)
                        .map(|m| (m.author_email.clone(), m.author_name.clone()))
                        .collect::<HashSet<_>>(),
                )
            } else {
                None
            };

            let commits =
                bound::git_log_commits_with_codeowners(since, until, directory, Some(memberships))?;
            let analysis = bound::analyze_by_contributor(commits, *adjusted)?;
            if *tsv {
                if *adjusted {
                    println!("author_name\tauthor_email\towner\tcommits\tchanges\tadjusted_commits\tadjusted_changes");
                } else {
                    println!("author_name\tauthor_email\towner\tcommits\tchanges");
                }
                for contributor_info in analysis {
                    if let Some(filter_authors) = &filter_authors {
                        if !filter_authors.contains(&(
                            Some(contributor_info.author_email.clone()),
                            Some(contributor_info.author_name.clone()),
                        )) {
                            continue;
                        }
                    }

                    for contribution in &contributor_info.contributions {
                        if *adjusted {
                            println!(
                                "{}\t{}\t{}\t{}\t{}\t{:.2}\t{}",
                                contributor_info.author_name,
                                contributor_info.author_email,
                                contribution.owner,
                                contribution.total_commits,
                                contribution.total_insertions + contribution.total_deletions,
                                contribution.adjusted_commits,
                                contribution.adjusted_changes
                            );
                        } else {
                            println!(
                                "{}\t{}\t{}\t{}\t{}",
                                contributor_info.author_name,
                                contributor_info.author_email,
                                contribution.owner,
                                contribution.total_commits,
                                contribution.total_insertions + contribution.total_deletions
                            );
                        }
                    }
                }
            } else {
                for contributor_info in analysis {
                    if let Some(filter_authors) = &filter_authors {
                        if !filter_authors.contains(&(
                            Some(contributor_info.author_email.clone()),
                            Some(contributor_info.author_name.clone()),
                        )) {
                            continue;
                        }
                    }

                    println!(
                        "Contributor: {} <{}>",
                        contributor_info.author_name, contributor_info.author_email
                    );
                    for contribution in &contributor_info.contributions {
                        println!("  Owner: {}", contribution.owner);
                        println!(
                            "    Changes: {} (+{}, -{})",
                            contribution.total_insertions + contribution.total_deletions,
                            contribution.total_insertions,
                            contribution.total_deletions
                        );
                        println!("    Commits: {}", contribution.total_commits);
                        if *adjusted {
                            println!("    Adjusted Changes: {}", contribution.adjusted_changes);
                            println!("    Adjusted Commits: {:.2}", contribution.adjusted_commits);
                        }
                    }
                    println!();
                }
            }
        }
    }

    Ok(())
}
