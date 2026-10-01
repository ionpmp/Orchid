//! Content extraction dispatch.
//!
//! The palette of extractors is pluggable: each implements
//! [`ContentExtractor`] and reports which MIME / extension combinations it
//! handles. [`Extractor`] picks one per file.

pub mod android;
pub mod audio;
pub mod bib;
pub mod cue;
pub mod docx;
pub mod eml;
pub mod epub_odf;
pub mod fb2;
pub mod feed;
pub mod gettext;
pub mod html;
pub mod ical;
pub mod info;
pub mod latex;
pub mod man;
pub mod mbox;
pub mod mo;
pub mod nfo;
pub mod notebook;
pub mod ooxml;
pub mod opml;
pub mod orchid;
pub mod pdf;
pub mod playlist;
pub mod pod;
pub mod properties;
pub mod qt;
pub mod rc;
pub mod reg;
pub mod resources;
pub mod rtf;
pub mod shortcut;
pub mod storyboard;
pub mod strings;
pub mod subtitle;
pub mod svg;
pub mod texinfo;
pub mod text;
pub mod ui;
pub mod unit;
pub mod wix;
pub mod xaml;
pub mod xspf;

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;

/// Per-format content extractor.
#[async_trait]
pub trait ContentExtractor: Send + Sync {
    /// Does this extractor cover the given MIME / extension?
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool;

    /// Extract textual content. May truncate to avoid huge strings.
    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String>;
}

/// Routes files to the first matching [`ContentExtractor`].
#[derive(Clone)]
pub struct Extractor {
    extractors: Vec<Arc<dyn ContentExtractor>>,
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor {
    /// Build a dispatcher with the built-in text extractor. PDF extraction
    /// requires pdfium at runtime and is opt-in via [`Extractor::with_pdf`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            // Calendar and contacts sit ahead of plain text so `text/calendar`
            // is not indexed as a raw blob (which would include PHOTO/ATTACH).
            extractors: vec![
                Arc::new(ical::IcsExtractor),
                Arc::new(ical::VcfExtractor),
                Arc::new(bib::BibExtractor),
                Arc::new(bib::RisExtractor),
                Arc::new(opml::OpmlExtractor),
                Arc::new(feed::FeedExtractor),
                Arc::new(notebook::NotebookExtractor),
                Arc::new(mbox::MboxExtractor),
                Arc::new(playlist::PlaylistExtractor),
                Arc::new(latex::LatexExtractor),
                Arc::new(cue::CueExtractor),
                Arc::new(gettext::GettextExtractor),
                Arc::new(shortcut::ShortcutExtractor),
                Arc::new(subtitle::SubtitleExtractor),
                Arc::new(nfo::NfoExtractor),
                Arc::new(svg::SvgExtractor),
                Arc::new(reg::RegExtractor),
                Arc::new(properties::PropertiesExtractor),
                Arc::new(resources::ResourceXmlExtractor),
                Arc::new(strings::StringsExtractor),
                Arc::new(qt::QtExtractor),
                Arc::new(rc::RcExtractor),
                Arc::new(ui::UiExtractor),
                Arc::new(xaml::XamlExtractor),
                Arc::new(man::ManExtractor),
                Arc::new(storyboard::StoryboardExtractor),
                Arc::new(pod::PodExtractor),
                Arc::new(texinfo::TexinfoExtractor),
                Arc::new(mo::MoExtractor),
                Arc::new(info::InfoExtractor),
                Arc::new(unit::UnitExtractor),
                Arc::new(xspf::XspfExtractor),
                Arc::new(wix::WixExtractor),
                Arc::new(text::TextExtractor),
            ],
        }
    }

    /// Plug in an extra extractor at the end of the chain.
    #[must_use]
    pub fn with(mut self, e: Arc<dyn ContentExtractor>) -> Self {
        self.extractors.push(e);
        self
    }

    /// Convenience: enable the PDF extractor.
    #[must_use]
    pub fn with_pdf(self) -> Self {
        self.with(Arc::new(pdf::PdfExtractor))
    }

    /// Convenience: enable the DOCX extractor.
    #[must_use]
    pub fn with_docx(self) -> Self {
        self.with(Arc::new(docx::DocxExtractor))
    }

    /// Convenience: enable Excel (`.xlsx` / `.xlsm`) text extraction.
    #[must_use]
    pub fn with_xlsx(self) -> Self {
        self.with(Arc::new(ooxml::XlsxExtractor))
    }

    /// Convenience: enable PowerPoint (`.pptx` / `.pptm` / `.ppsx`) text extraction.
    #[must_use]
    pub fn with_pptx(self) -> Self {
        self.with(Arc::new(ooxml::PptxExtractor))
    }

    /// Convenience: enable EPUB chapter text extraction.
    #[must_use]
    pub fn with_epub(self) -> Self {
        self.with(Arc::new(epub_odf::EpubExtractor))
    }

    /// Convenience: enable OpenDocument (`.odt` / `.ods` / `.odp`) text extraction.
    #[must_use]
    pub fn with_odf(self) -> Self {
        self.with(Arc::new(epub_odf::OdfExtractor))
    }

    /// Convenience: enable Rich Text (`.rtf`) extraction.
    #[must_use]
    pub fn with_rtf(self) -> Self {
        self.with(Arc::new(rtf::RtfExtractor))
    }

    /// Convenience: enable ID3 and Vorbis-comment extraction for audio files.
    #[must_use]
    pub fn with_audio(self) -> Self {
        self.with(Arc::new(audio::AudioTagExtractor))
    }

    /// Enable `.eml` extraction ahead of the plain-text fallback.
    #[must_use]
    pub fn with_eml(mut self) -> Self {
        self.extractors.insert(0, Arc::new(eml::EmlExtractor));
        self
    }

    /// Enable FictionBook extraction ahead of the plain-text XML fallback.
    ///
    /// `.fb2.zip` is recognized by file name in [`Extractor::extract`].
    #[must_use]
    pub fn with_fb2(mut self) -> Self {
        self.extractors.insert(0, Arc::new(fb2::Fb2Extractor));
        self
    }

    /// Enable HTML text extraction ahead of the plain-text fallback.
    ///
    /// Inserted at the front so `.html` is not indexed as raw markup.
    #[must_use]
    pub fn with_html(mut self) -> Self {
        self.extractors.insert(0, Arc::new(html::HtmlTextExtractor));
        self
    }

    /// Convenience: enable the `.orchid` Clean-Text extractor.
    #[must_use]
    pub fn with_orchid(self) -> Self {
        self.with(Arc::new(orchid::OrchidExtractor))
    }

    /// Route `path` to a matching extractor, returning `None` when none
    /// applies.
    pub async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
        mime: Option<&str>,
    ) -> Result<Option<String>> {
        let name = path.file_name().unwrap_or("");
        if name.to_ascii_lowercase().ends_with(".fb2.zip") {
            for e in &self.extractors {
                if e.can_handle(None, Some("fb2")) {
                    return Ok(Some(e.extract(provider, path).await?));
                }
            }
        }
        let extension = path.extension().map(|e| e.to_ascii_lowercase());
        // Feeds are often saved as `.xml`, which the plain-text extractor
        // would otherwise index as markup.
        if extension.as_deref() == Some("xml") {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            let decoded = text::decode_best_effort(&raw);
            if feed::looks_like_feed(&decoded) {
                return Ok(Some(feed::feed_text(&decoded)));
            }
            if android::looks_like_android(&decoded) {
                return Ok(Some(android::android_text(&decoded)));
            }
            return Ok(Some(decoded));
        }
        for e in &self.extractors {
            if e.can_handle(mime, extension.as_deref()) {
                return Ok(Some(e.extract(provider, path).await?));
            }
        }
        Ok(None)
    }
}

impl std::fmt::Debug for Extractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extractor")
            .field("extractors", &self.extractors.len())
            .finish()
    }
}
