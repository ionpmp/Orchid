//! `.orchid` Clean-Text reaches the Tantivy index via OrchidExtractor.

use std::sync::Arc;

use orchid_format::{write_sealed_file, SealedCreateRequest, MIME_TYPE};
use orchid_fs::{FsPath, FsProvider, FsProviderRegistry, LocalProvider};
use orchid_search::{DocumentKind, Extractor, IndexDocument, QueryBuilder, SearchEngine};

#[tokio::test]
async fn orchid_extractor_feeds_search_content() {
    let td = tempfile::tempdir().unwrap();
    let orchid_path = td.path().join("note.orchid");
    let clean = "Phase5 live index should find this unique orchid phrase xyzzy42.";
    write_sealed_file(
        &orchid_path,
        &SealedCreateRequest {
            file_uuid: Some([0x77; 16]),
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
            embeddings: None,
        },
    )
    .unwrap();

    let registry = Arc::new(FsProviderRegistry::new());
    registry
        .register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>)
        .unwrap();
    let fs_path = FsPath::from_local(&orchid_path).unwrap();
    let provider = registry.for_path(&fs_path).expect("local provider");

    let extractor = Extractor::new().with_orchid();
    let text = extractor
        .extract(provider.as_ref(), &fs_path, Some(MIME_TYPE))
        .await
        .unwrap()
        .expect("orchid extractor should match");
    assert!(text.contains("xyzzy42"));

    let engine = SearchEngine::open(td.path().join("idx").as_path()).unwrap();
    engine
        .upsert(IndexDocument {
            path: fs_path.to_string(),
            name: "note.orchid".into(),
            extension: Some("orchid".into()),
            content: Some(text),
            tags: vec![],
            color_label: None,
            size: clean.len() as u64,
            modified: 1,
            mime: Some(MIME_TYPE.into()),
            kind: DocumentKind::File,
            in_archive: None,
            embedding: None,
        })
        .await
        .unwrap();
    engine.commit().await.unwrap();

    let hits = engine
        .search(QueryBuilder::new().text("xyzzy42").build())
        .await
        .unwrap();
    assert!(
        hits.hits.iter().any(|h| h.name == "note.orchid"),
        "expected note.orchid hit; got {:?}",
        hits.hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
}
