//! Full-text search for Orchid backed by Tantivy.
//!
//! Components:
//!
//! * [`SearchEngine`] — Tantivy index + writer + reader facade.
//! * [`indexer`] — scheduler + FS-event subscriber + content-extractor
//!   dispatch.
//! * [`extractors`] — text, PDF, DOCX, and `.orchid` extractors.
//! * [`query`] — query builder and result types.
//! * [`ann`] / [`hybrid`] — Phase 5 embedding ANN + BM25 fusion
//!   ([`SearchEngine::search_hybrid`] keeps the ANN in lockstep with upserts).

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod ann;
pub mod engine;
pub mod error;
pub mod extractors;
pub mod hybrid;
pub mod indexer;
pub mod query;
pub mod schema;

pub use ann::AnnIndex;
pub use engine::{DocumentKind, IndexDocument, SearchEngine};
pub use error::{Result, SearchError};
pub use extractors::{ContentExtractor, Extractor};
pub use hybrid::{hybrid_search, semantic_search};
pub use indexer::crawl_roots;
pub use indexer::{scheduler::IndexTask, watcher::IndexScope, IndexFsSubscriber, IndexScheduler};
pub use query::{Query, QueryBuilder, SearchHit, SearchResults, Snippet};
pub use schema::Schema;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
