//! Global registry mapping paths / schemes / ids to concrete providers.

use std::sync::Arc;

use dashmap::DashMap;

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::{FsProvider, ProviderId};

/// In-memory provider registry.
#[derive(Default)]
pub struct FsProviderRegistry {
    providers: DashMap<ProviderId, Arc<dyn FsProvider>>,
    by_scheme: DashMap<String, ProviderId>,
}

impl std::fmt::Debug for FsProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsProviderRegistry")
            .field("providers", &self.providers.len())
            .field("schemes", &self.by_scheme.len())
            .finish()
    }
}

impl FsProviderRegistry {
    /// Build an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider. If no default is yet set for the provider's
    /// scheme, the newcomer becomes the default.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ProviderNotFound`] if an id collision is detected
    /// (reused for brevity; in future versions this will be its own variant).
    pub fn register(&self, provider: Arc<dyn FsProvider>) -> Result<()> {
        let id = provider.id().clone();
        if self.providers.contains_key(&id) {
            return Err(FsError::ProviderNotFound(format!(
                "duplicate provider id: {id}"
            )));
        }
        let scheme = provider.scheme().to_string();
        self.providers.insert(id.clone(), provider);
        // Set default for the scheme if none exists yet.
        self.by_scheme.entry(scheme).or_insert(id);
        Ok(())
    }

    /// Remove a provider by id.
    pub fn unregister(&self, id: &ProviderId) -> bool {
        let Some((_, removed)) = self.providers.remove(id) else {
            return false;
        };
        let scheme = removed.scheme();
        // If that id was the scheme default, try to pick another.
        let should_replace = self
            .by_scheme
            .get(scheme)
            .map(|v| v.value() == id)
            .unwrap_or(false);
        if should_replace {
            self.by_scheme.remove(scheme);
            // Find any remaining provider for the scheme.
            if let Some(other) = self
                .providers
                .iter()
                .find(|e| e.value().scheme() == scheme)
                .map(|e| e.key().clone())
            {
                self.by_scheme.insert(scheme.to_string(), other);
            }
        }
        true
    }

    /// Look up by id.
    #[must_use]
    pub fn get(&self, id: &ProviderId) -> Option<Arc<dyn FsProvider>> {
        self.providers.get(id).map(|e| Arc::clone(e.value()))
    }

    /// Resolve the provider responsible for `path`'s scheme.
    #[must_use]
    pub fn for_path(&self, path: &FsPath) -> Option<Arc<dyn FsProvider>> {
        let id = self.by_scheme.get(path.scheme()).map(|e| e.value().clone())?;
        self.get(&id)
    }

    /// List every registered provider id.
    #[must_use]
    pub fn list_providers(&self) -> Vec<ProviderId> {
        self.providers.iter().map(|e| e.key().clone()).collect()
    }

    /// Change the default provider for a scheme.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ProviderNotFound`] if `id` is not registered.
    pub fn set_default_for_scheme(&self, scheme: &str, id: &ProviderId) -> Result<()> {
        if !self.providers.contains_key(id) {
            return Err(FsError::ProviderNotFound(id.to_string()));
        }
        self.by_scheme.insert(scheme.to_string(), id.clone());
        Ok(())
    }
}
