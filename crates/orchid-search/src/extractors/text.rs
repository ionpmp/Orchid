//! Plain-text extractor with encoding detection.

use async_trait::async_trait;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

use crate::error::Result;
use crate::extractors::ContentExtractor;

/// Maximum number of indexed content bytes per document.
pub const MAX_CONTENT_BYTES: usize = 2 * 1024 * 1024;

/// Default text-ish extensions.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "text", "log", "md", "markdown", "rst", "adoc", "org", "csv", "tsv", "json", "jsonl",
    "ndjson", "xml", "html", "htm", "ini", "toml", "yaml", "yml", "conf", "cfg", "env", "rs", "py",
    "pyi", "js", "mjs", "cjs", "jsx", "ts", "tsx", "css", "scss", "less", "c", "h", "cpp", "hpp",
    "cc", "hh", "cs", "java", "kt", "kts", "go", "rb", "php", "swift", "lua", "pl", "sh", "bash",
    "zsh", "ps1", "psm1", "bat", "cmd", "sql", "vue", "svelte", "dart", "ex", "exs", "erl", "hs",
    "ml", "cmake", "mk", "ftl", "slint", "gradle",
];

/// Extract readable text from plaintext-ish files.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextExtractor;

#[async_trait]
impl ContentExtractor for TextExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        // Subtitles are timed text. Leave them for [`super::subtitle`].
        // `.nfo` is either Kodi XML or a scene note. Leave it for [`super::nfo`].
        // `.svg` labels are extracted separately. Leave them for [`super::svg`].
        // `.reg` string values are extracted separately. Leave them for [`super::reg`].
        // `.properties` escapes are decoded separately. Leave them for [`super::properties`].
        // `.resx` and XLIFF strings are extracted separately. Leave them for [`super::resources`].
        // Apple string tables are extracted separately. Leave them for [`super::strings`].
        // Windows resource scripts are extracted separately. Leave them for [`super::rc`].
        // Interface files are extracted separately. Leave them for [`super::ui`].
        // XAML labels are extracted separately. Leave them for [`super::xaml`].
        // Storyboards are extracted separately. Leave them for [`super::storyboard`].
        // Perl POD is extracted separately. Leave it for [`super::pod`].
        // Texinfo is extracted separately. Leave it for [`super::texinfo`].
        // Compiled gettext catalogs are extracted separately. Leave them for [`super::mo`].
        // Manual pages are extracted separately. Leave them for [`super::man`].
        // GNU Info is extracted separately. Leave it for [`super::info`].
        // systemd units are extracted separately. Leave them for [`super::unit`].
        // XSPF playlists are extracted separately. Leave them for [`super::xspf`].
        // WiX sources are extracted separately. Leave them for [`super::wix`].
        // Torrent metainfo is extracted separately. Leave it for [`super::torrent`].
        // RPM specs are extracted separately. Leave them for [`super::spec`].
        // Debian control files are extracted separately. Leave them for [`super::debian`].
        // RDoc is extracted separately. Leave it for [`super::rdoc`].
        // MSBuild projects are extracted separately. Leave them for [`super::msbuild`].
        // Unified diffs are extracted separately. Leave them for [`super::diff`].
        // NuGet manifests are extracted separately. Leave them for [`super::nuspec`].
        // Visual Studio solutions are extracted separately. Leave them for [`super::sln`].
        // XML solutions are extracted separately. Leave them for [`super::slnx`].
        if extension.is_some_and(super::man::is_man_ext)
            || extension.is_some_and(super::info::is_info_ext)
            || extension.is_some_and(super::unit::is_unit_ext)
        {
            return false;
        }
        if extension.is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "srt"
                    | "vtt"
                    | "ass"
                    | "ssa"
                    | "lrc"
                    | "smi"
                    | "ttml"
                    | "dfxp"
                    | "nfo"
                    | "svg"
                    | "reg"
                    | "properties"
                    | "resx"
                    | "xlf"
                    | "xliff"
                    | "strings"
                    | "stringsdict"
                    | "rc"
                    | "ui"
                    | "xaml"
                    | "storyboard"
                    | "xib"
                    | "pod"
                    | "texi"
                    | "texinfo"
                    | "txi"
                    | "mo"
                    | "gmo"
                    | "xspf"
                    | "wxs"
                    | "wxl"
                    | "torrent"
                    | "spec"
                    | "dsc"
                    | "changes"
                    | "rdoc"
                    | "csproj"
                    | "fsproj"
                    | "vbproj"
                    | "vcxproj"
                    | "diff"
                    | "patch"
                    | "nuspec"
                    | "sln"
                    | "slnx"
            )
        }) {
            return false;
        }
        if let Some(m) = mime {
            let base = m.split(';').next().unwrap_or(m).trim();
            // `.rtf` is `text/rtf` but is not plain text. Leave it for [`super::rtf`].
            if base.eq_ignore_ascii_case("text/rtf")
                || base.eq_ignore_ascii_case("text/vtt")
                || base.eq_ignore_ascii_case("application/x-subrip")
                || base.eq_ignore_ascii_case("application/x-srt")
                || base.eq_ignore_ascii_case("text/x-ssa")
                || base.eq_ignore_ascii_case("application/x-ass")
                || base.eq_ignore_ascii_case("text/x-lrc")
            {
                return false;
            }
            if base.starts_with("text/") || base == "application/json" || base == "application/xml"
            {
                return true;
            }
        }
        if let Some(ext) = extension {
            let lower = ext.to_ascii_lowercase();
            return TEXT_EXTENSIONS.iter().any(|e| *e == lower);
        }
        false
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        // Cap the read at the index budget — avoid loading a 50 MiB log just to
        // keep the first 2 MiB.
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(decode_best_effort(&raw))
    }
}

pub(crate) fn decode_best_effort(bytes: &[u8]) -> String {
    // UTF-8 BOM takes priority.
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let s = String::from_utf8_lossy(&bytes[3..]).into_owned();
        return s;
    }
    // Let chardetng decide based on the prefix.
    let mut det = EncodingDetector::new(Iso2022JpDetection::Allow);
    let head = if bytes.len() > 4 * 1024 {
        &bytes[..4 * 1024]
    } else {
        bytes
    };
    det.feed(head, head.len() == bytes.len());
    let encoding = det.guess(None, Utf8Detection::Allow);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bom_is_stripped_and_content_preserved() {
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice("hello world".as_bytes());
        let decoded = decode_best_effort(&raw);
        assert_eq!(decoded, "hello world");
    }

    #[test]
    fn latin1_falls_back_to_windows_1252() {
        let raw = b"caf\xe9"; // "café" in Windows-1252
        let decoded = decode_best_effort(raw);
        assert!(decoded.contains("caf"));
    }

    #[test]
    fn handler_matches_extension() {
        let e = TextExtractor;
        assert!(e.can_handle(None, Some("md")));
        assert!(e.can_handle(None, Some("rs")));
        assert!(e.can_handle(None, Some("ps1")));
        assert!(!e.can_handle(None, Some("ass")));
        assert!(!e.can_handle(Some("text/plain"), Some("srt")));
        assert!(e.can_handle(Some("text/plain"), None));
        assert!(!e.can_handle(Some("image/png"), Some("png")));
        assert!(!e.can_handle(None, Some("exe")));
    }
}
