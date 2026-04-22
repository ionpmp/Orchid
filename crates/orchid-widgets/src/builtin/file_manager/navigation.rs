//! Navigation helper: lists a directory via the provider registry and
//! builds breadcrumb segments.

use std::sync::Arc;

/// One step in a breadcrumb trail.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct BreadcrumbSegment {
    pub path: orchid_fs::FsPath,
    pub display_name: String,
}

/// Result of a navigation request.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct NavigationResult {
    pub entries: Vec<orchid_fs::FsEntry>,
    pub breadcrumbs: Vec<BreadcrumbSegment>,
    pub parent: Option<orchid_fs::FsPath>,
    pub total_entries: usize,
    pub error: Option<String>,
}

/// Listing helper.
pub struct Navigator {
    registry: Arc<orchid_fs::FsProviderRegistry>,
}

impl std::fmt::Debug for Navigator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Navigator").finish_non_exhaustive()
    }
}

impl Navigator {
    /// Build a navigator over the given provider registry.
    #[must_use]
    pub fn new(registry: Arc<orchid_fs::FsProviderRegistry>) -> Self {
        Self { registry }
    }

    /// List `path`. On failure the result carries the error in `error`
    /// and returns an empty entry list.
    pub async fn navigate(
        &self,
        path: &orchid_fs::FsPath,
        show_hidden: bool,
    ) -> NavigationResult {
        let Some(provider) = self.registry.for_path(path) else {
            return NavigationResult {
                entries: Vec::new(),
                breadcrumbs: self.breadcrumbs_for(path),
                parent: path.parent(),
                total_entries: 0,
                error: Some(format!("no provider for scheme `{}`", path.scheme())),
            };
        };
        let mut entries = match provider.list(path).await {
            Ok(entries) => entries,
            Err(e) => {
                return NavigationResult {
                    entries: Vec::new(),
                    breadcrumbs: self.breadcrumbs_for(path),
                    parent: path.parent(),
                    total_entries: 0,
                    error: Some(e.to_string()),
                };
            }
        };
        if !show_hidden {
            entries.retain(|e| !e.metadata.hidden);
        }
        let total = entries.len();
        NavigationResult {
            entries,
            breadcrumbs: self.breadcrumbs_for(path),
            parent: path.parent(),
            total_entries: total,
            error: None,
        }
    }

    /// Compute breadcrumb segments for `path` by walking its parents.
    #[must_use]
    pub fn breadcrumbs_for(&self, path: &orchid_fs::FsPath) -> Vec<BreadcrumbSegment> {
        let mut trail = Vec::new();
        let mut cursor = Some(path.clone());
        while let Some(p) = cursor {
            let display = p.file_name().map(String::from).unwrap_or_else(|| p.scheme().to_string());
            trail.push(BreadcrumbSegment {
                path: p.clone(),
                display_name: display,
            });
            cursor = p.parent();
        }
        trail.reverse();
        trail
    }
}
