//! Query builder and result types.

pub mod builder;
pub mod snippet;

pub use builder::{Query, QueryBuilder};
pub use snippet::{SearchHit, SearchResults, Snippet};
