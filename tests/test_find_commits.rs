use bound::{find_first_commit_on_or_after_date, find_last_commit_before_date};
use chrono::{DateTime, TimeZone, Utc};
use git2::Repository;
use std::path::PathBuf;

#[test]
fn test_find_first_commit_after_date() {
    // Test case for early date
    let early_date = Utc.with_ymd_and_hms(1970, 2, 1, 0, 0, 0).unwrap();
    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_repo_early");
    let repo = Repository::init(&repo_path).unwrap();

    create_commit(&repo, "Very early commit", "1970-01-01T00:00:00Z");
    create_commit(&repo, "Early commit", "1970-03-01T00:00:00Z");

    let result_early = find_first_commit_on_or_after_date(&repo, early_date).unwrap();
    assert!(result_early.is_some());
    assert_eq!(result_early.unwrap().message().unwrap(), "Early commit");

    std::fs::remove_dir_all(repo_path).unwrap();

    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_repo");
    let repo = Repository::init(&repo_path).unwrap();

    // Create some test commits
    create_commit(&repo, "Initial commit", "2023-01-01T00:00:00Z");
    create_commit(&repo, "Second commit", "2023-02-01T00:00:00Z");
    create_commit(&repo, "Third commit", "2023-03-01T00:00:00Z");

    // Test case 1: Find commit after 2023-01-15
    let date1 = Utc.with_ymd_and_hms(2023, 1, 15, 0, 0, 0).unwrap();
    let result1 = find_first_commit_on_or_after_date(&repo, date1).unwrap();
    assert!(result1.is_some());
    assert_eq!(result1.unwrap().message().unwrap(), "Second commit");

    // Test case 2: Find commit after 2023-02-15
    let date2 = Utc.with_ymd_and_hms(2023, 2, 15, 0, 0, 0).unwrap();
    let result2 = find_first_commit_on_or_after_date(&repo, date2).unwrap();
    assert!(result2.is_some());
    assert_eq!(result2.unwrap().message().unwrap(), "Third commit");

    // Test case 3: No commit after 2023-03-15
    let date3 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let result3 = find_first_commit_on_or_after_date(&repo, date3).unwrap();
    assert!(result3.is_none());

    // Clean up the test repository
    std::fs::remove_dir_all(repo_path).unwrap();
}

fn create_commit(repo: &Repository, message: &str, date: &str) {
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    let parent_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    let time = DateTime::parse_from_rfc3339(date).unwrap();
    let git_time = git2::Time::new(time.timestamp(), 0);
    let signature = git2::Signature::new("Test User", "test@example.com", &git_time).unwrap();

    let parents = parent_commit.as_ref().map(|c| vec![c]).unwrap_or_default();

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        parents.as_slice(),
    )
    .unwrap();
}

#[test]
fn test_find_last_commit_before_date() {
    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_repo_before");
    let repo = Repository::init(&repo_path).unwrap();

    // Create some test commits
    create_commit(&repo, "Initial commit", "2023-01-01T00:00:00Z");
    create_commit(&repo, "Second commit", "2023-02-01T00:00:00Z");
    create_commit(&repo, "Third commit", "2023-03-01T00:00:00Z");

    // Test case 1: Find commit before 2023-02-15
    let date1 = Utc.with_ymd_and_hms(2023, 2, 15, 0, 0, 0).unwrap();
    let result1 = find_last_commit_before_date(&repo, date1).unwrap();
    assert!(result1.is_some());
    assert_eq!(result1.unwrap().message().unwrap(), "Second commit");

    // Test case 2: Find commit before 2023-01-15
    let date2 = Utc.with_ymd_and_hms(2023, 1, 15, 0, 0, 0).unwrap();
    let result2 = find_last_commit_before_date(&repo, date2).unwrap();
    assert!(result2.is_some());
    assert_eq!(result2.unwrap().message().unwrap(), "Initial commit");

    // Test case 3: Find commit before 2022-12-31 (before all commits)
    let date3 = Utc.with_ymd_and_hms(2022, 12, 31, 0, 0, 0).unwrap();
    let result3 = find_last_commit_before_date(&repo, date3).unwrap();
    assert!(result3.is_none());

    // Test case 4: Find commit before 2023-03-15 (after all commits)
    let date4 = Utc.with_ymd_and_hms(2023, 3, 15, 0, 0, 0).unwrap();
    let result4 = find_last_commit_before_date(&repo, date4).unwrap();
    assert!(result4.is_some());
    assert_eq!(result4.unwrap().message().unwrap(), "Third commit");

    // Clean up the test repository
    std::fs::remove_dir_all(repo_path).unwrap();
}
