mod co;
mod commits;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BoundError {
    #[error("Failed to execute git command: {0}")]
    GitExecutionError(#[from] std::io::Error),
    #[error("Failed to parse UTF-8: {0}")]
    UTF8Error(#[from] std::string::FromUtf8Error),
    #[error("Expected <EMPTY LINE>, got: {0}")]
    UnexpectedLineError(String),
    #[error("Expected COMMIT, got: {0}")]
    UnexpectedCommitError(String),
    #[error("Failed to parse CODEOWNERS file: {0}")]
    CodeownersParseError(String),
}

pub use co::{
    get_codeowners_at_commit, git_log_commits_with_codeowners, AuthorCodeownerMemberships,
    CommitInfoWithCodeowner, CommitsWithCodeownersIterator, FileChangeWithCodeowner,
};
pub use commits::{git_log_commits, CommitAuthor, CommitInfo, FileChange, GitLogLinesIterator};
