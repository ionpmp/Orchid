//! Word navigation / deletion for the DOCX preview caret.

use orchid_viewers::document::model::{Block, Document as Doc, Paragraph, Run, RunStyle};
use orchid_viewers::document::DocumentViewer;

#[test]
fn preview_word_move_and_word_delete() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hello brave world".into(),
                style: RunStyle::default(),
            }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    // Caret at start of "brave" (byte 6).
    viewer.set_selection_plain_offsets(6, 6);
    viewer.preview_move_by_words(1, false);
    assert_eq!(viewer.selection().head.byte_offset, 12); // start of "world"
    viewer.preview_move_by_words(1, false);
    assert_eq!(viewer.selection().head.byte_offset, 17); // end
    viewer.preview_move_by_words(-1, false);
    assert_eq!(viewer.selection().head.byte_offset, 12); // start of "world"
    viewer.preview_move_by_words(-1, true);
    assert_eq!(viewer.selected_plain_text(), "brave ");

    viewer.set_selection_plain_offsets(17, 17);
    viewer.preview_delete_word_backward().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hello brave ");
    }
    viewer.set_selection_plain_offsets(6, 6);
    viewer.preview_delete_word_forward().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hello ");
    }
}

#[test]
fn preview_select_word_at_offset() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hello brave world".into(),
                style: RunStyle::default(),
            }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    // Mid-"brave" → select whole word.
    viewer.preview_select_word_at_offset(8);
    assert_eq!(viewer.selected_plain_text(), "brave");
    // On the space after "Hello" → snap into preceding word.
    viewer.preview_select_word_at_offset(5);
    assert_eq!(viewer.selected_plain_text(), "Hello");
    // Punctuation run.
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hi!!! there".into(),
                style: RunStyle::default(),
            }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.preview_select_word_at_offset(3);
    assert_eq!(viewer.selected_plain_text(), "!!!");
}
