//! Smoke tests for orchid-viewers public surfaces.

use std::io::Write;
use std::time::Instant;

use orchid_viewers::thumbnail::generator::image_thumbnail;
use orchid_viewers::{
    kind_for, LineEnding, TextBuffer, TextOp, TextOpKind, ThumbnailSize, UndoStack, ViewerError,
    ViewerKind,
};

fn path(s: &str) -> orchid_fs::FsPath {
    orchid_fs::FsPath::new(s).expect("valid FsPath")
}

#[test]
fn dispatch_kind_for_common_samples() {
    assert_eq!(
        kind_for(&path("local:/a/photo.png"), b"\x89PNG\r\n\x1a\n"),
        Some(ViewerKind::Image)
    );
    assert_eq!(
        kind_for(&path("local:/a/doc.pdf"), b"%PDF-1.7"),
        Some(ViewerKind::Pdf)
    );
    assert_eq!(
        kind_for(&path("local:/a/notes.md"), b"# hello"),
        Some(ViewerKind::Text)
    );
    assert_eq!(
        kind_for(&path("local:/a/bundle.zip"), b"PK\x03\x04"),
        Some(ViewerKind::Archive)
    );
    assert_eq!(
        kind_for(&path("local:/a/shot.heic"), b""),
        Some(ViewerKind::Image)
    );
}

#[test]
fn text_buffer_edit_dirty_and_roundtrip() {
    let mut buf = TextBuffer::from_bytes(b"hello\nworld\n").unwrap();
    assert!(buf.line_count() >= 2);
    assert!(!buf.is_dirty());
    assert_eq!(buf.line_ending(), LineEnding::Lf);

    buf.insert(0, 5, "!").unwrap();
    assert!(buf.is_dirty());
    assert_eq!(buf.line(0).as_deref(), Some("hello!"));

    let bytes = buf.to_bytes().unwrap();
    assert!(String::from_utf8(bytes).unwrap().starts_with("hello!"));
}

#[test]
fn undo_stack_push_undo_redo() {
    let mut stack = UndoStack::new(0);
    stack.push(TextOp {
        kind: TextOpKind::Insert,
        start_line: 0,
        start_column: 0,
        end_line: 0,
        end_column: 1,
        text: "a".into(),
        timestamp: Instant::now(),
    });
    stack.push(TextOp {
        kind: TextOpKind::Insert,
        start_line: 0,
        start_column: 1,
        end_line: 0,
        end_column: 2,
        text: "b".into(),
        timestamp: Instant::now(),
    });
    assert_eq!(stack.len(), 2);
    assert!(stack.undo().is_some());
    assert!(stack.redo().is_some());
}

#[test]
fn image_thumbnail_from_tiny_png() {
    // Minimal valid 1×1 PNG (red pixel).
    let png: &[u8] = include_bytes!("fixtures/1x1.png");
    let thumb = image_thumbnail(png, ThumbnailSize::Small.to_pixels()).unwrap();
    assert!(thumb.width > 0);
    assert!(thumb.height > 0);
    assert!(!thumb.rgba.is_empty());
}

#[test]
fn unsupported_image_errors_expose_ftl_keys() {
    assert_eq!(
        ViewerError::UnsupportedHeic.to_string(),
        "viewer-image-heic-unsupported"
    );
    assert_eq!(
        ViewerError::UnsupportedRaw.to_string(),
        "viewer-image-raw-unsupported"
    );
}

#[tokio::test]
async fn archive_list_zip_fixture() {
    let td = tempfile::tempdir().unwrap();
    let zip_path = td.path().join("fixture.zip");
    {
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("readme.txt", opts).unwrap();
        zw.write_all(b"hello from zip").unwrap();
        zw.finish().unwrap();
    }
    let reader = orchid_fs::open_archive(&zip_path).unwrap();
    let entries = reader.list().await.unwrap();
    assert!(
        entries.iter().any(|e| e.path.contains("readme")),
        "entries: {entries:?}"
    );
}
