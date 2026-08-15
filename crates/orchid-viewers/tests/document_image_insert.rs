//! Insert inline images into the DOCX preview model.

use orchid_viewers::document::model::Block;
use orchid_viewers::document::DocumentViewer;

fn tiny_png() -> Vec<u8> {
    let mut png = Vec::new();
    let img = image::RgbaImage::from_pixel(6, 3, image::Rgba([9, 8, 7, 255]));
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

#[test]
fn preview_insert_image_after_caret_block() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(orchid_viewers::document::model::Document {
        blocks: vec![Block::Paragraph(
            orchid_viewers::document::model::Paragraph {
                runs: vec![orchid_viewers::document::model::Run {
                    text: "Hello".into(),
                    style: Default::default(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(5, 5);
    let png = tiny_png();
    viewer.preview_insert_image(png.clone(), 6, 3).unwrap();
    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match &doc.blocks[1] {
        Block::Image(img) => {
            assert_eq!(img.bytes, png);
            assert_eq!(img.width_px, 6);
            assert_eq!(img.height_px, 3);
        }
        _ => panic!("expected image block"),
    }
    assert_eq!(viewer.selection().head.block_idx, 1);
}
