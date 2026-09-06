//! Phase 5 DONE: semantic / hybrid retrieval finds a `.orchid` BM25 misses.

use orchid_embed::{Embedder, StubEmbedder};
use orchid_format::{
    document_embedding, write_sealed_file, SealedCreateRequest, MIME_TYPE,
};
use orchid_search::{
    hybrid_search, semantic_search, AnnIndex, DocumentKind, IndexDocument, QueryBuilder,
    SearchEngine,
};

fn approx_tokens(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

#[tokio::test]
async fn semantic_query_finds_orchid_that_bm25_misses() {
    let td = tempfile::tempdir().unwrap();
    let orchid_path = td.path().join("dog.orchid");
    let clean = "The friendly dog plays outside in the park every morning.";
    let embedder = StubEmbedder::new();
    let vector = embedder.embed(clean).unwrap();
    let emb = document_embedding(
        embedder.model_id(),
        clean.len() as u64,
        approx_tokens(clean),
        vector.clone(),
    );

    write_sealed_file(
        &orchid_path,
        &SealedCreateRequest {
            file_uuid: Some([0x66; 16]),
            created_unix_ms: Some(1),
            raw: Vec::new(),
            raw_content_type: None,
            raw_name: None,
            clean_text: clean.as_bytes().to_vec(),
            structured: b"{}".to_vec(),
            structured_content_type: None,
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings: Some(emb),
        },
    )
    .unwrap();

    let path_key = format!("local:{}", orchid_path.display());
    let engine = SearchEngine::open(td.path().join("idx").as_path()).unwrap();
    engine
        .upsert(IndexDocument {
            path: path_key.clone(),
            name: "dog.orchid".into(),
            extension: Some("orchid".into()),
            content: Some(clean.to_string()),
            tags: vec![],
            color_label: None,
            size: clean.len() as u64,
            modified: 1_700_000_000,
            mime: Some(MIME_TYPE.into()),
            kind: DocumentKind::File,
            in_archive: None,
        })
        .await
        .unwrap();
    engine.commit().await.unwrap();

    // Distractor text sharing no synonym concepts with the query.
    let distractor = "Quarterly revenue spreadsheet and invoice totals.";
    let mut ann = AnnIndex::new(embedder.dimensions());
    ann.upsert(&path_key, vector).unwrap();
    ann.upsert(
        "local:/other/finance.txt",
        embedder.embed(distractor).unwrap(),
    )
    .unwrap();

    // Pure BM25: "canine" is absent from Clean-Text → no hit.
    let bm25 = engine
        .search(QueryBuilder::new().text("canine").limit(10).build())
        .await
        .unwrap();
    assert!(
        bm25.hits.iter().all(|h| h.path != path_key),
        "BM25 should miss synonym-only query; hits={:?}",
        bm25.hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );

    // ANN alone ranks the dog document first.
    let semantic = semantic_search(&ann, &embedder, "canine companion", 5).unwrap();
    assert_eq!(semantic[0].0, path_key);

    // Hybrid fusion still surfaces the `.orchid` document.
    let hybrid = hybrid_search(&engine, &ann, &embedder, "canine companion", 10)
        .await
        .unwrap();
    assert!(
        hybrid.hits.iter().any(|h| h.path == path_key),
        "hybrid should retrieve .orchid; hits={:?}",
        hybrid.hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );
}
