use std::{collections::HashMap, io};

use crate::{CommitInfoWithCodeowner, FileChangeWithCodeowner};

pub struct ContributorToOwnerInfo {
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
    pub adjusted_changes_by_team: usize,
    pub adjusted_commits_by_team: f64,
    pub adjusted_changes_by_others: usize,
    pub adjusted_commits_by_others: f64,
    pub top_outside_contributors_by_changes: Vec<ContributorToOwnerInfo>,
    pub top_outside_contributors_by_commits: Vec<ContributorToOwnerInfo>,
    pub top_team_contributors_by_changes: Vec<ContributorToOwnerInfo>,
    pub top_team_contributors_by_commits: Vec<ContributorToOwnerInfo>,
}

pub fn analyze_by_owner(
    commits: impl Iterator<Item = Result<CommitInfoWithCodeowner, io::Error>>,
    adjusted: bool,
) -> Result<Vec<OwnerInfo>, io::Error> {
    let mut owners: HashMap<String, OwnerInfo> = HashMap::new();

    let mut team_contributors: HashMap<String, HashMap<(String, String), (usize, usize)>> =
        HashMap::new();
    let mut outside_contributors: HashMap<String, HashMap<(String, String), (usize, usize)>> =
        HashMap::new();

    for commit_result in commits {
        let commit = commit_result?;
        let mut commit_total_insertions: usize = 0;
        let mut commit_changes_by_owner: HashMap<String, usize> = HashMap::new();

        // First pass: calculate total insertions for this commit
        for change in &commit.file_changes {
            if let Some(codeowners) = &change.codeowners {
                for owner in codeowners {
                    *commit_changes_by_owner.entry(owner.clone()).or_insert(0) +=
                        change.insertions as usize;
                    commit_total_insertions += change.insertions as usize;
                }
            }
        }

        // Second pass: update metrics
        for change in &commit.file_changes {
            if let Some(codeowners) = &change.codeowners {
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
                        adjusted_changes_by_team: 0,
                        adjusted_commits_by_team: 0.0,
                        adjusted_changes_by_others: 0,
                        adjusted_commits_by_others: 0.0,
                    });

                    let is_team_member = change.author_is_codeowner.unwrap_or(false);
                    if is_team_member {
                        owner_info.total_insertions_by_team += change.insertions as usize;
                        owner_info.total_deletions_by_team += change.deletions as usize;
                        owner_info.total_commits_by_team += 1;
                        if adjusted {
                            owner_info.adjusted_changes_by_team += change.insertions as usize;
                            let commit_weight = if commit_total_insertions > 0 {
                                *commit_changes_by_owner.get(owner).unwrap_or(&0) as f64
                                    / commit_total_insertions as f64
                            } else {
                                0.0
                            };
                            owner_info.adjusted_commits_by_team += commit_weight;
                        }
                        update_contributor_stats(&mut team_contributors, owner, &commit, &change);
                    } else {
                        owner_info.total_insertions_by_others += change.insertions as usize;
                        owner_info.total_deletions_by_others += change.deletions as usize;
                        owner_info.total_commits_by_others += 1;
                        if adjusted {
                            owner_info.adjusted_changes_by_others += change.insertions as usize;
                            let commit_weight = if commit_total_insertions > 0 {
                                *commit_changes_by_owner.get(owner).unwrap_or(&0) as f64
                                    / commit_total_insertions as f64
                            } else {
                                0.0
                            };
                            owner_info.adjusted_commits_by_others += commit_weight;
                        }
                        update_contributor_stats(
                            &mut outside_contributors,
                            owner,
                            &commit,
                            &change,
                        );
                    }
                }
            }
        }
    }

    // Process contributors and update OwnerInfo
    for (owner, owner_info) in owners.iter_mut() {
        update_top_contributors(owner_info, &team_contributors.get(owner), true);
        update_top_contributors(owner_info, &outside_contributors.get(owner), false);
    }

    let mut sorted_owners: Vec<OwnerInfo> = owners.into_values().collect();
    sorted_owners.sort_by(|a, b| a.owner.cmp(&b.owner));
    Ok(sorted_owners)
}

fn update_contributor_stats(
    contributors: &mut HashMap<String, HashMap<(String, String), (usize, usize)>>,
    owner: &str,
    commit: &CommitInfoWithCodeowner,
    change: &FileChangeWithCodeowner,
) {
    let owner_contributors = contributors.entry(owner.to_string()).or_default();
    let contributor_key = (commit.author_name.clone(), commit.author_email.clone());
    let (changes, commits) = owner_contributors.entry(contributor_key).or_insert((0, 0));
    *changes += change.insertions as usize + change.deletions as usize;
    *commits += 1;
}

fn update_top_contributors(
    owner_info: &mut OwnerInfo,
    contributors: &Option<&HashMap<(String, String), (usize, usize)>>,
    is_team: bool,
) {
    if let Some(contributors) = contributors {
        let mut contributors: Vec<_> = contributors.iter().collect();
        contributors.sort_by(|(_, (changes_a, _)), (_, (changes_b, _))| changes_b.cmp(changes_a));
        let top_by_changes: Vec<ContributorToOwnerInfo> = contributors
            .iter()
            .take(10)
            .map(|((name, email), (changes, _))| ContributorToOwnerInfo {
                author_name: name.clone(),
                author_email: email.clone(),
                metric_value: *changes,
            })
            .collect();

        contributors.sort_by(|(_, (_, commits_a)), (_, (_, commits_b))| commits_b.cmp(commits_a));
        let top_by_commits: Vec<ContributorToOwnerInfo> = contributors
            .iter()
            .take(10)
            .map(|((name, email), (_, commits))| ContributorToOwnerInfo {
                author_name: name.clone(),
                author_email: email.clone(),
                metric_value: *commits,
            })
            .collect();

        if is_team {
            owner_info.top_team_contributors_by_changes = top_by_changes;
            owner_info.top_team_contributors_by_commits = top_by_commits;
        } else {
            owner_info.top_outside_contributors_by_changes = top_by_changes;
            owner_info.top_outside_contributors_by_commits = top_by_commits;
        }
    }
}
pub struct ContributionsByOwnerInfo {
    pub owner: String,
    pub total_insertions: usize,
    pub total_deletions: usize,
    pub total_commits: usize,
    pub adjusted_changes: usize,
    pub adjusted_commits: f64,
}

pub struct ContributorInfo {
    pub author_name: String,
    pub author_email: String,
    pub contributions: Vec<ContributionsByOwnerInfo>,
}

pub fn analyze_by_contributor(
    commits: impl Iterator<Item = Result<CommitInfoWithCodeowner, io::Error>>,
    adjusted: bool,
) -> Result<Vec<ContributorInfo>, io::Error> {
    let mut contributors: HashMap<(String, String), Vec<ContributionsByOwnerInfo>> = HashMap::new();

    for commit_result in commits {
        let commit = commit_result?;
        let contributor_key = (commit.author_name.clone(), commit.author_email.clone());

        let mut commit_total_insertions: usize = 0;
        let mut commit_changes_by_owner: HashMap<String, usize> = HashMap::new();

        // First pass: calculate total insertions for this commit
        for change in &commit.file_changes {
            let owner = match &change.codeowners {
                Some(codeowners) if !codeowners.is_empty() => codeowners[0].clone(),
                _ => "<unowned>".to_string(),
            };
            *commit_changes_by_owner.entry(owner).or_insert(0) += change.insertions as usize;
            commit_total_insertions += change.insertions as usize;
        }

        // Second pass: update metrics
        for change in &commit.file_changes {
            let owner = match &change.codeowners {
                Some(codeowners) if !codeowners.is_empty() => codeowners[0].clone(),
                _ => "<unowned>".to_string(),
            };

            let contributions = contributors
                .entry(contributor_key.clone())
                .or_insert_with(Vec::new);
            if let Some(contribution) = contributions.iter_mut().find(|c| c.owner == owner) {
                contribution.total_insertions += change.insertions as usize;
                contribution.total_deletions += change.deletions as usize;
                contribution.total_commits += 1;
                if adjusted {
                    contribution.adjusted_changes += change.insertions as usize;
                    let commit_weight = if commit_total_insertions > 0 {
                        *commit_changes_by_owner.get(&owner).unwrap_or(&0) as f64
                            / commit_total_insertions as f64
                    } else {
                        0.0
                    };
                    contribution.adjusted_commits += commit_weight;
                }
            } else {
                contributions.push(ContributionsByOwnerInfo {
                    owner: owner.clone(),
                    total_insertions: change.insertions as usize,
                    total_deletions: change.deletions as usize,
                    total_commits: 1,
                    adjusted_changes: if adjusted {
                        change.insertions as usize
                    } else {
                        0
                    },
                    adjusted_commits: if adjusted {
                        if commit_total_insertions > 0 {
                            *commit_changes_by_owner.get(&owner).unwrap_or(&0) as f64
                                / commit_total_insertions as f64
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    },
                });
            }
        }
    }

    let mut result: Vec<ContributorInfo> = contributors
        .into_iter()
        .map(|((author_name, author_email), mut contributions)| {
            contributions.sort_by(|a, b| b.total_commits.cmp(&a.total_commits));
            ContributorInfo {
                author_name,
                author_email,
                contributions,
            }
        })
        .collect();

    result.sort_by(|a, b| a.author_name.cmp(&b.author_name));

    Ok(result)
}
