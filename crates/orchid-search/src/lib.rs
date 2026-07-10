//! Full-text search for Orchid backed by Tantivy.
//!
//! Components:
//!
//! * [`SearchEngine`] — Tantivy index + writer + reader facade.
//! * [`indexer`] — scheduler + FS-event subscriber + content-extractor
//!   dispatch.
//! * [`extractors`] — text and PDF extractors.
//! * [`query`] — query builder and result types.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod engine;
pub mod error;
pub mod extractors;
pub mod indexer;
pub mod query;
pub mod schema;

pub use engine::{DocumentKind, IndexDocument, SearchEngine};
pub use error::{Result, SearchError};
pub use extractors::{ContentExtractor, Extractor};
pub use indexer::{scheduler::IndexTask, watcher::IndexScope, IndexFsSubscriber, IndexScheduler};
pub use indexer::crawl_roots;
pub use query::{Query, QueryBuilder, SearchHit, SearchResults, Snippet};
pub use schema::Schema;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
