//! Content extraction dispatch.
//!
//! The palette of extractors is pluggable: each implements
//! [`ContentExtractor`] and reports which MIME / extension combinations it
//! handles. [`Extractor`] picks one per file.

pub mod android;
pub mod audio;
pub mod bib;
pub mod bun;
pub mod cargo;
pub mod cartfile;
pub mod composer;
pub mod cue;
pub mod debian;
pub mod deno;
pub mod diff;
pub mod docker;
pub mod docx;
pub mod eml;
pub mod epub_odf;
pub mod fb2;
pub mod feed;
pub mod flake;
pub mod gemfile;
pub mod gettext;
pub mod gomod;
pub mod gosum;
pub mod html;
pub mod ical;
pub mod info;
pub mod latex;
pub mod man;
pub mod manifest;
pub mod mbox;
pub mod mix;
pub mod mo;
pub mod msbuild;
pub mod nfo;
pub mod notebook;
pub mod nugetlock;
pub mod nuspec;
pub mod ooxml;
pub mod opml;
pub mod orchid;
pub mod pdm;
pub mod pdf;
pub mod pipfile;
pub mod pkg;
pub mod pkglock;
pub mod playlist;
pub mod plist;
pub mod pnpm;
pub mod pod;
pub mod podfile;
pub mod poetry;
pub mod pom;
pub mod properties;
pub mod pubspec;
pub mod qt;
pub mod rc;
pub mod rdoc;
pub mod reg;
pub mod resolved;
pub mod resources;
pub mod rtf;
pub mod shortcut;
pub mod sln;
pub mod slnx;
pub mod spec;
pub mod storyboard;
pub mod strings;
pub mod subtitle;
pub mod svg;
pub mod terraform;
pub mod texinfo;
pub mod text;
pub mod torrent;
pub mod ui;
pub mod unit;
pub mod uv;
pub mod wix;
pub mod xaml;
pub mod xspf;
pub mod yarn;

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
                Arc::new(torrent::TorrentExtractor),
                Arc::new(spec::SpecExtractor),
                Arc::new(debian::DebianExtractor),
                Arc::new(rdoc::RdocExtractor),
                Arc::new(msbuild::MsbuildExtractor),
                Arc::new(diff::DiffExtractor),
                Arc::new(nuspec::NuspecExtractor),
                Arc::new(sln::SlnExtractor),
                Arc::new(slnx::SlnxExtractor),
                Arc::new(plist::PlistExtractor),
                Arc::new(docker::DockerExtractor),
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
        // `Dockerfile` has no extension, so `text/plain` would index `ENV` values.
        if docker::needs_name_dispatch(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(docker::docker_text(&text::decode_best_effort(&raw))));
        }
        // `package.json` is plain JSON, which would index script commands.
        if pkg::is_pkg_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(pkg::pkg_text(&text::decode_best_effort(&raw))));
        }
        // `composer.lock` is plain JSON, which would index dist checksums.
        if composer::is_composer_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(composer::composer_lock_text(
                &text::decode_best_effort(&raw),
            )));
        }
        // `.terraform.lock.hcl` is plain text, which would index provider hashes.
        if terraform::is_terraform_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(terraform::terraform_lock_text(
                &text::decode_best_effort(&raw),
            )));
        }
        // `Cartfile.resolved` is plain text, which would index commits.
        if cartfile::is_cartfile_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(cartfile::cartfile_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `Package.resolved` is plain JSON, which would index revisions.
        if resolved::is_resolved_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(resolved::resolved_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `deno.lock` is plain JSON, which would index integrity hashes.
        if deno::is_deno_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(deno::deno_lock_text(&text::decode_best_effort(&raw))));
        }
        // `bun.lock` is plain JSON, which would index integrity hashes.
        if bun::is_bun_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(bun::bun_lock_text(&text::decode_best_effort(&raw))));
        }
        // `flake.lock` is plain JSON, which would index narHash values and revisions.
        if flake::is_flake_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(flake::flake_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `mix.lock` is plain text, which would index checksums.
        if mix::is_mix_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(mix::mix_lock_text(&text::decode_best_effort(&raw))));
        }
        // `pdm.lock` is plain text, which would index file hashes.
        if pdm::is_pdm_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(pdm::pdm_lock_text(&text::decode_best_effort(&raw))));
        }
        // `Podfile.lock` is plain text, which would index checksums and commits.
        if podfile::is_podfile_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(podfile::podfile_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `packages.lock.json` is plain JSON, which would index content hashes.
        if nugetlock::is_nuget_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(nugetlock::nuget_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `uv.lock` is plain text, which would index package hashes.
        if uv::is_uv_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(uv::uv_lock_text(&text::decode_best_effort(&raw))));
        }
        // `pubspec.lock` is plain YAML, which would index sha256 checksums.
        if pubspec::is_pubspec_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(pubspec::pubspec_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `Pipfile.lock` is plain JSON, which would index package hashes.
        if pipfile::is_pipfile_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(pipfile::pipfile_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `Gemfile.lock` is plain text, which would index revisions and checksums.
        if gemfile::is_gemfile_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(gemfile::gemfile_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `poetry.lock` is plain text, which would index file hashes.
        if poetry::is_poetry_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(poetry::poetry_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
        // `pnpm-lock.yaml` is plain text, which would index integrity hashes.
        if pnpm::is_pnpm_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(pnpm::pnpm_text(&text::decode_best_effort(&raw))));
        }
        // `yarn.lock` is plain text, which would index integrity hashes.
        if yarn::is_yarn_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(yarn::yarn_text(&text::decode_best_effort(&raw))));
        }
        // `package-lock.json` is plain JSON, which would index integrity hashes.
        if pkglock::is_pkglock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(pkglock::pkglock_text(&text::decode_best_effort(&raw))));
        }
        // `go.mod` is plain text, which would index dependency versions.
        if gomod::is_gomod_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(gomod::gomod_text(&text::decode_best_effort(&raw))));
        }
        // `go.sum` is plain text, which would index checksum lines.
        if gosum::is_gosum_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(gosum::gosum_text(&text::decode_best_effort(&raw))));
        }
        // `Cargo.lock` is plain text, which would index checksums.
        if cargo::is_cargo_lock_name(name) {
            let raw = orchid_fs::read_prefix(provider, path, text::MAX_CONTENT_BYTES).await?;
            return Ok(Some(cargo::cargo_lock_text(&text::decode_best_effort(
                &raw,
            ))));
        }
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
            if pom::looks_like_pom(&decoded) {
                return Ok(Some(pom::pom_text(&decoded)));
            }
            if manifest::looks_like_manifest(&decoded) {
                return Ok(Some(manifest::manifest_text(&decoded)));
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
