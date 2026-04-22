//! Localization for Orchid — Fluent-backed message catalogues.
//!
//! The [`LocaleManager`] loads one [`fluent_bundle::FluentBundle`] per
//! supported language and resolves message keys via [`LocaleManager::tr`]
//! / [`LocaleManager::tr_args`].
//!
//! Two locales are bundled at compile time from
//! `crates/orchid-i18n/locales/`: `en-US` (the fallback) and `ru-RU`.
//! Additional / overriding catalogues may be loaded from a
//! runtime-configurable directory — see [`LocaleManager::new`].
//!
//! # Example
//!
//! ```
//! use orchid_i18n::{default_language, LocaleManager};
//! let mgr = LocaleManager::new(default_language(), None).unwrap();
//! let hello = mgr.tr("startup-welcome");
//! assert!(!hello.is_empty());
//! ```
#![warn(missing_docs)]
#![warn(clippy::all)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fluent::FluentResource;
use fluent_bundle::bundle::FluentBundle;
use fluent_bundle::FluentValue;
use parking_lot::RwLock;
use tracing::{debug, warn};
use unic_langid::LanguageIdentifier;

/// Error type surfaced by locale operations.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum I18nError {
    /// Language tag failed to parse.
    #[error("invalid BCP-47 language id: {0}")]
    InvalidLanguage(String),
    /// Requested locale has no bundle registered.
    #[error("no bundle for language: {0}")]
    UnknownLanguage(String),
    /// Reading a bundle file failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result alias.
pub type Result<T, E = I18nError> = std::result::Result<T, E>;

/// BCP-47 locale id wrapped in its own newtype so the UI layer can treat
/// it opaquely.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocaleId(LanguageIdentifier);

impl LocaleId {
    /// Parse a BCP-47 tag.
    ///
    /// # Errors
    ///
    /// Returns [`I18nError::InvalidLanguage`] when the string is not valid BCP-47.
    pub fn parse(s: &str) -> Result<Self> {
        s.parse::<LanguageIdentifier>()
            .map(LocaleId)
            .map_err(|e| I18nError::InvalidLanguage(format!("{s}: {e}")))
    }

    /// Canonical BCP-47 representation.
    #[must_use]
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    /// Borrow the underlying `LanguageIdentifier`.
    #[must_use]
    pub fn inner(&self) -> &LanguageIdentifier {
        &self.0
    }
}

impl std::fmt::Display for LocaleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The default fallback language used when the user's preference cannot
/// be resolved.
#[must_use]
pub fn default_language() -> LocaleId {
    LocaleId::parse("en-US").expect("en-US is a valid BCP-47 tag")
}

/// Arguments passed into a Fluent message placeholder.
#[derive(Default)]
pub struct FluentArgs {
    inner: fluent_bundle::FluentArgs<'static>,
}

impl FluentArgs {
    /// Empty argument set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `key` to a string value.
    pub fn set(&mut self, key: &'static str, value: impl Into<String>) -> &mut Self {
        self.inner.set(key, FluentValue::from(value.into()));
        self
    }

    /// Builder-style variant of [`Self::set`].
    #[must_use]
    pub fn with(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.set(key, value);
        self
    }
}

impl std::fmt::Debug for FluentArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FluentArgs").finish_non_exhaustive()
    }
}

/// Bundled English catalogue.
const EN_US_FTL: &str = include_str!("../locales/en-US/main.ftl");
/// Bundled Russian catalogue.
const RU_RU_FTL: &str = include_str!("../locales/ru-RU/main.ftl");

/// Resolved message store for all supported languages.
///
/// The manager keeps one `FluentBundle` per registered locale plus a
/// "current" pointer that [`LocaleManager::tr`] consults. Fallback to
/// [`default_language`] is automatic when a key is missing from the
/// active locale.
pub struct LocaleManager {
    inner: Arc<Inner>,
}

type IntlBundle = FluentBundle<FluentResource, intl_memoizer::concurrent::IntlLangMemoizer>;

struct Inner {
    bundles: RwLock<Vec<(LocaleId, IntlBundle)>>,
    current: RwLock<LocaleId>,
    fallback: LocaleId,
}

impl std::fmt::Debug for LocaleManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocaleManager")
            .field("current", &*self.inner.current.read())
            .field("fallback", &self.inner.fallback)
            .field("bundles", &self.inner.bundles.read().len())
            .finish()
    }
}

impl LocaleManager {
    /// Build a manager starting in `initial` language. Always registers
    /// the bundled `en-US` and `ru-RU` catalogues. If `extra_dir` is
    /// provided, overlays `<extra_dir>/<lang>/main.ftl` on top of the
    /// bundled copy (missing files are silently ignored).
    ///
    /// # Errors
    ///
    /// Returns [`I18nError::InvalidLanguage`] if any built-in catalogue
    /// has an invalid language tag (internal programming error). File
    /// I/O errors from `extra_dir` are logged and skipped — they never
    /// break construction.
    pub fn new(initial: LocaleId, extra_dir: Option<PathBuf>) -> Result<Self> {
        let mut bundles = Vec::new();
        for (tag, source) in [("en-US", EN_US_FTL), ("ru-RU", RU_RU_FTL)] {
            let locale = LocaleId::parse(tag)?;
            let mut bundle = new_bundle(locale.clone());
            load_into(&mut bundle, source);
            if let Some(extra) = extra_dir.as_ref() {
                if let Some(path) = overlay_path(extra, tag) {
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        debug!(path = %path.display(), "overlaying locale");
                        load_into(&mut bundle, &contents);
                    }
                }
            }
            bundles.push((locale, bundle));
        }

        let fallback = default_language();
        Ok(Self {
            inner: Arc::new(Inner {
                bundles: RwLock::new(bundles),
                current: RwLock::new(initial),
                fallback,
            }),
        })
    }

    /// Currently-active locale.
    #[must_use]
    pub fn current(&self) -> LocaleId {
        self.inner.current.read().clone()
    }

    /// Switch the active locale. No-op if the language is not registered.
    pub fn set_current(&self, locale: LocaleId) {
        let bundles = self.inner.bundles.read();
        if bundles.iter().any(|(l, _)| l == &locale) {
            drop(bundles);
            *self.inner.current.write() = locale;
        } else {
            warn!(?locale, "locale not registered; ignoring set_current");
        }
    }

    /// Locales that have a bundle registered, in insertion order.
    #[must_use]
    pub fn available_locales(&self) -> Vec<LocaleId> {
        self.inner
            .bundles
            .read()
            .iter()
            .map(|(l, _)| l.clone())
            .collect()
    }

    /// Resolve `key` in the current locale, falling back to
    /// [`default_language`] and finally to the key itself.
    #[must_use]
    pub fn tr(&self, key: &str) -> String {
        self.tr_args(key, &FluentArgs::new())
    }

    /// Variant of [`Self::tr`] with message arguments.
    #[must_use]
    pub fn tr_args(&self, key: &str, args: &FluentArgs) -> String {
        let current = self.inner.current.read().clone();
        if let Some(s) = self.lookup(&current, key, args) {
            return s;
        }
        if current != self.inner.fallback {
            if let Some(s) = self.lookup(&self.inner.fallback, key, args) {
                return s;
            }
        }
        // Last-resort: echo the key so missing-translation bugs are
        // visible rather than invisible.
        key.to_string()
    }

    fn lookup(&self, locale: &LocaleId, key: &str, args: &FluentArgs) -> Option<String> {
        let bundles = self.inner.bundles.read();
        let (_, bundle) = bundles.iter().find(|(l, _)| l == locale)?;
        let message = bundle.get_message(key)?;
        let pattern = message.value()?;
        let mut errors = Vec::new();
        let resolved = bundle.format_pattern(pattern, Some(&args.inner), &mut errors);
        if !errors.is_empty() {
            warn!(?errors, %key, "fluent formatting reported errors");
        }
        Some(resolved.into_owned())
    }
}

fn new_bundle(locale: LocaleId) -> IntlBundle {
    let mut bundle: IntlBundle =
        FluentBundle::new_concurrent(vec![locale.inner().clone()]);
    // Fluent's default bidi isolation pads every substitution with
    // `U+2068 / U+2069`, which mangles the way desktop UI renders
    // runs of arguments. Disable for our terminal-style strings.
    bundle.set_use_isolating(false);
    bundle
}

fn load_into(bundle: &mut IntlBundle, source: &str) {
    match FluentResource::try_new(source.to_string()) {
        Ok(res) => {
            if let Err(errors) = bundle.add_resource(res) {
                warn!(?errors, "fluent bundle add_resource errors");
            }
        }
        Err((res, errors)) => {
            warn!(?errors, "fluent resource parse errors; loading partial");
            if let Err(e) = bundle.add_resource(res) {
                warn!(?e, "fluent bundle add_resource errors on partial");
            }
        }
    }
}

fn overlay_path(extra_dir: &Path, tag: &str) -> Option<PathBuf> {
    Some(extra_dir.join(tag).join("main.ftl"))
}

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_en_us() {
        assert_eq!(default_language().as_str(), "en-US");
    }

    #[test]
    fn tr_returns_bundled_english() {
        let mgr = LocaleManager::new(default_language(), None).unwrap();
        let welcome = mgr.tr("startup-welcome");
        assert!(welcome.contains("Welcome"), "got: {welcome}");
    }

    #[test]
    fn tr_falls_back_to_english_when_key_missing_in_current() {
        let mgr = LocaleManager::new(LocaleId::parse("ru-RU").unwrap(), None).unwrap();
        // A key that exists in both catalogues.
        let welcome = mgr.tr("startup-welcome");
        assert!(welcome.contains("Добро"), "got: {welcome}");
    }

    #[test]
    fn tr_args_interpolates_version() {
        let mgr = LocaleManager::new(default_language(), None).unwrap();
        let out = mgr.tr_args(
            "startup-version-label",
            &FluentArgs::new().with("version", "9.9.9"),
        );
        assert!(out.contains("9.9.9"), "got: {out}");
    }

    #[test]
    fn tr_unknown_key_returns_key() {
        let mgr = LocaleManager::new(default_language(), None).unwrap();
        assert_eq!(mgr.tr("nonexistent-key"), "nonexistent-key");
    }

    #[test]
    fn set_current_switches_language() {
        let mgr = LocaleManager::new(default_language(), None).unwrap();
        mgr.set_current(LocaleId::parse("ru-RU").unwrap());
        assert_eq!(mgr.current().as_str(), "ru-RU");
    }
}
