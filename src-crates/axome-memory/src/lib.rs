//! Axome's long-term memory: Markdown notes with typed facts and links,
//! indexed into a derived SQLite file for search.

pub mod edit;
pub mod format;
pub mod index;
pub mod links;
pub mod permalink;
pub mod search;
pub mod sync;
pub mod temporal;

pub use index::Index;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;
