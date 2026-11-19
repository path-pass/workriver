//! What workriver reads from git: a unit's commits with each file's text before and after,
//! the uncommitted work on a branch, and a repository's file tree. Plain `git` processes.
mod files;
mod history;
mod runner;

pub use files::*;
pub use history::*;
pub use runner::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Git(String),
    #[error("cannot run git: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;
