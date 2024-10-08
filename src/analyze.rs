use std::{collections::HashMap, io};

use crate::CommitInfoWithCodeowner;

pub struct ContributorInfo {
    pub author_name: String,
    pub author_email: String,

    pub metric_value: usize,
}

pub struct OwnerInfo {
    pub owner: String,
    pub total_insertions_by_team: usize,
    pub total_deletions_by_team: usize,
    pub total_commits_by_team: usize,

    pub total_insertions_by_others: usize,
    pub total_deletions_by_others: usize,
    pub total_commits_by_others: usize,

    pub top_outside_contributors_by_changes: Vec<ContributorInfo>,
    pub top_outside_contributors_by_commits: Vec<ContributorInfo>,
    pub top_team_contributors_by_changes: Vec<ContributorInfo>,
    pub top_team_contributors_by_commits: Vec<ContributorInfo>,
}

pub fn analyze_by_owner(
    commits: impl Iterator<Item = Result<CommitInfoWithCodeowner, io::Error>>,
) -> Result<Vec<OwnerInfo>, io::Error> {
    let mut owners: HashMap<String, OwnerInfo> = HashMap::new();
    let mut contributors: HashMap<(String, String), (usize, usize)> = HashMap::new();

    for commit_result in commits {
        let commit = commit_result?;
        for change in commit.file_changes {
            if let Some(codeowners) = change.codeowners {
                for owner in codeowners {
                    let owner_info = owners.entry(owner.clone()).or_insert_with(|| OwnerInfo {
                        owner: owner.clone(),
                        total_insertions_by_team: 0,
                        total_deletions_by_team: 0,
                        total_commits_by_team: 0,
                        total_insertions_by_others: 0,
                        total_deletions_by_others: 0,
                        total_commits_by_others: 0,
                        top_outside_contributors_by_changes: Vec::new(),
                        top_outside_contributors_by_commits: Vec::new(),
                        top_team_contributors_by_changes: Vec::new(),
                        top_team_contributors_by_commits: Vec::new(),
                    });

                    let is_team_member = change.author_is_codeowner.unwrap_or(false);

                    if is_team_member {
                        owner_info.total_insertions_by_team += change.insertions as usize;
                        owner_info.total_deletions_by_team += change.deletions as usize;
                        owner_info.total_commits_by_team += 1;
                    } else {
                        owner_info.total_insertions_by_others += change.insertions as usize;
                        owner_info.total_deletions_by_others += change.deletions as usize;
                        owner_info.total_commits_by_others += 1;
                    }

                    let contributor_key = (commit.author_name.clone(), commit.author_email.clone());
                    let (changes, commits) = contributors.entry(contributor_key).or_insert((0, 0));
                    *changes += change.insertions as usize + change.deletions as usize;
                    *commits += 1;
                }
            }
        }
    }

    // Process contributors and update OwnerInfo
    for owner_info in owners.values_mut() {
        let mut team_contributors: Vec<_> = contributors
            .iter()
            .filter(|((name, email), _)| {
                owner_info
                    .top_team_contributors_by_changes
                    .iter()
                    .any(|c| &c.author_name == name && &c.author_email == email)
            })
            .collect();
        let mut outside_contributors: Vec<_> = contributors
            .iter()
            .filter(|((name, email), _)| {
                !owner_info
                    .top_team_contributors_by_changes
                    .iter()
                    .any(|c| &c.author_name == name && &c.author_email == email)
            })
            .collect();

        team_contributors
            .sort_by(|(_, (changes_a, _)), (_, (changes_b, _))| changes_b.cmp(changes_a));
        outside_contributors
            .sort_by(|(_, (changes_a, _)), (_, (changes_b, _))| changes_b.cmp(changes_a));

        owner_info.top_team_contributors_by_changes = team_contributors
            .iter()
            .take(10)
            .map(|((name, email), (changes, _))| ContributorInfo {
                author_name: name.clone(),
                author_email: email.clone(),
                metric_value: *changes,
            })
            .collect();

        owner_info.top_outside_contributors_by_changes = outside_contributors
            .iter()
            .take(10)
            .map(|((name, email), (changes, _))| ContributorInfo {
                author_name: name.clone(),
                author_email: email.clone(),
                metric_value: *changes,
            })
            .collect();

        team_contributors
            .sort_by(|(_, (_, commits_a)), (_, (_, commits_b))| commits_b.cmp(commits_a));
        outside_contributors
            .sort_by(|(_, (_, commits_a)), (_, (_, commits_b))| commits_b.cmp(commits_a));

        owner_info.top_team_contributors_by_commits = team_contributors
            .iter()
            .take(10)
            .map(|((name, email), (_, commits))| ContributorInfo {
                author_name: name.clone(),
                author_email: email.clone(),
                metric_value: *commits,
            })
            .collect();

        owner_info.top_outside_contributors_by_commits = outside_contributors
            .iter()
            .take(10)
            .map(|((name, email), (_, commits))| ContributorInfo {
                author_name: name.clone(),
                author_email: email.clone(),
                metric_value: *commits,
            })
            .collect();
    }

    let mut sorted_owners: Vec<OwnerInfo> = owners.into_values().collect();
    sorted_owners.sort_by(|a, b| a.owner.cmp(&b.owner));
    Ok(sorted_owners)
}
