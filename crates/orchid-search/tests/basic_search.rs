//! Basic index + search round-trip.

use orchid_search::{DocumentKind, IndexDocument, QueryBuilder, SearchEngine};

fn mk_doc(path: &str, name: &str, ext: &str, content: Option<&str>, tags: Vec<&str>) -> IndexDocument {
    IndexDocument {
        path: path.to_string(),
        name: name.to_string(),
        extension: Some(ext.to_string()),
        content: content.map(str::to_owned),
        tags: tags.into_iter().map(|s| s.to_string()).collect(),
        color_label: None,
        size: (content.map(str::len).unwrap_or(0)) as u64,
        modified: 1_700_000_000,
        mime: None,
        kind: DocumentKind::File,
        in_archive: None,
    }
}

#[tokio::test]
async fn upsert_then_search_by_name_and_extension() {
    let td = tempfile::tempdir().unwrap();
    let engine = SearchEngine::open(td.path()).unwrap();

    let mut docs: Vec<IndexDocument> = Vec::new();
    for i in 0..10 {
        docs.push(mk_doc(
            &format!("local:/work/report-{i}.pdf"),
            &format!("report-{i}.pdf"),
            "pdf",
            Some(&format!("quarterly revenue summary {i}")),
            vec!["work", "quarterly"],
        ));
    }
    for i in 0..10 {
        docs.push(mk_doc(
            &format!("local:/work/memo-{i}.md"),
            &format!("memo-{i}.md"),
            "md",
            Some(&format!("personal note number {i}")),
            vec!["personal"],
        ));
    }
    engine.upsert_batch(docs).await.unwrap();
    engine.commit().await.unwrap();

    // Text search across name/content.
    let q = QueryBuilder::new().text("quarterly").build();
    let hits = engine.search(q).await.unwrap();
    assert!(!hits.hits.is_empty());
    assert!(hits.hits.iter().all(|h| h.path.contains("report-")));
    let with_snippet = hits
        .hits
        .iter()
        .find_map(|h| h.snippet.as_ref())
        .expect("content match should produce a snippet");
    assert!(
        with_snippet.text.to_ascii_lowercase().contains("quarterly"),
        "snippet text was: {}",
        with_snippet.text
    );
    assert!(
        !with_snippet.highlights.is_empty(),
        "snippet should highlight the query term"
    );

    // Extension filter alone.
    let q = QueryBuilder::new().extension("md").build();
    let hits = engine.search(q).await.unwrap();
    assert_eq!(hits.hits.len(), 10);
    assert!(hits.hits.iter().all(|h| h.extension.as_deref() == Some("md")));

    // Doc count.
    assert_eq!(engine.doc_count().unwrap(), 20);
}

#[tokio::test]
async fn remove_drops_from_doc_count() {
    let td = tempfile::tempdir().unwrap();
    let engine = SearchEngine::open(td.path()).unwrap();

    engine.upsert(mk_doc("local:/a.txt", "a.txt", "txt", Some("alpha"), vec![])).await.unwrap();
    engine.upsert(mk_doc("local:/b.txt", "b.txt", "txt", Some("beta"), vec![])).await.unwrap();
    engine.commit().await.unwrap();
    assert_eq!(engine.doc_count().unwrap(), 2);

    engine.remove("local:/a.txt").await.unwrap();
    engine.commit().await.unwrap();
    assert_eq!(engine.doc_count().unwrap(), 1);
}
