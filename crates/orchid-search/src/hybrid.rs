//! Hybrid BM25 + ANN retrieval (reciprocal rank fusion).

use orchid_embed::Embedder;

use crate::ann::AnnIndex;
use crate::engine::{DocumentKind, SearchEngine};
use crate::error::{Result, SearchError};
use crate::query::builder::Query;
use crate::query::snippet::{SearchHit, SearchResults};

/// Constant for reciprocal rank fusion.
const RRF_K: f32 = 60.0;

/// Run BM25 and ANN, fuse with reciprocal rank fusion, return top hits.
pub async fn hybrid_search(
    engine: &SearchEngine,
    ann: &AnnIndex,
    embedder: &dyn Embedder,
    text: &str,
    limit: usize,
) -> Result<SearchResults> {
    let started = std::time::Instant::now();
    let limit = limit.max(1);
    let bm25 = engine
        .search(Query {
            text: Some(text.to_string()),
            limit: limit.max(50),
            ..Query::empty()
        })
        .await?;

    let qvec = embedder.embed(text).map_err(|e| SearchError::Extraction {
        path: String::new(),
        reason: format!("embed query: {e}"),
    })?;
    let ann_hits = ann.search(&qvec, limit.max(50));

    Ok(fuse_rrf(bm25, &ann_hits, limit, started))
}

/// Reciprocal-rank fusion of a BM25 result set and ANN `(path, score)` hits.
pub(crate) fn fuse_rrf(
    bm25: SearchResults,
    ann_hits: &[(String, f32)],
    limit: usize,
    started: std::time::Instant,
) -> SearchResults {
    let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for (rank, hit) in bm25.hits.iter().enumerate() {
        *scores.entry(hit.path.clone()).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    for (rank, (path, _sim)) in ann_hits.iter().enumerate() {
        *scores.entry(path.clone()).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
    }

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);

    let bm25_by_path: std::collections::HashMap<&str, &SearchHit> =
        bm25.hits.iter().map(|h| (h.path.as_str(), h)).collect();

    let hits: Vec<SearchHit> = ranked
        .into_iter()
        .map(|(path, score)| {
            if let Some(h) = bm25_by_path.get(path.as_str()) {
                let mut out = (*h).clone();
                out.score = score;
                out
            } else {
                SearchHit {
                    path: path.clone(),
                    name: path
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(path.as_str())
                        .to_string(),
                    extension: Some("orchid".into()),
                    size: 0,
                    modified: 0,
                    mime: Some(orchid_format::MIME_TYPE.into()),
                    kind: DocumentKind::File,
                    score,
                    snippet: None,
                }
            }
        })
        .collect();

    SearchResults {
        hits,
        total_estimated: 0,
        query_time_ms: started.elapsed().as_millis() as u64,
    }
}

/// ANN-only top-k (no BM25). Useful for proving semantic-only retrieval.
pub fn semantic_search(
    ann: &AnnIndex,
    embedder: &dyn Embedder,
    text: &str,
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    let qvec = embedder.embed(text).map_err(|e| SearchError::Extraction {
        path: String::new(),
        reason: format!("embed query: {e}"),
    })?;
    Ok(ann.search(&qvec, limit.max(1)))
}
