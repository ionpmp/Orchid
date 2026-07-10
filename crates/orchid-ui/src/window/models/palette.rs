//! Command palette candidate model builder.

use orchid_core::{CommandDescriptor, CommandPalette, CommandRegistry};
use orchid_i18n::LocaleManager;
use slint::SharedString;

use crate::slint_generated::SearchCandidateEntry;

pub(crate) fn build_palette_candidates(
    palette: &CommandPalette,
    registry: &CommandRegistry,
    locale: &LocaleManager,
    query: &str,
    limit: usize,
) -> Vec<SearchCandidateEntry> {
    if query.trim().is_empty() {
        palette
            .browse()
            .into_iter()
            .take(limit)
            .map(|desc| palette_entry_from_descriptor(&desc, registry, locale))
            .collect()
    } else {
        palette
            .search(query, limit)
            .into_iter()
            .map(|hit| palette_entry_from_descriptor(&hit.descriptor, registry, locale))
            .collect()
    }
}

fn palette_entry_from_descriptor(
    desc: &CommandDescriptor,
    registry: &CommandRegistry,
    locale: &LocaleManager,
) -> SearchCandidateEntry {
    let shortcut = registry
        .effective_shortcut(&desc.id)
        .or(desc.default_shortcut.clone())
        .map(|s| s.to_string());
    let subtitle: SharedString = desc
        .terminal_invocation
        .as_ref()
        .map(|t| {
            locale
                .tr_args(
                    "command-terminal-invocation",
                    &orchid_i18n::FluentArgs::new().with("verb", t.verb.as_str()),
                )
                .into()
        })
        .unwrap_or_default();
    SearchCandidateEntry {
        id: desc.id.clone().into(),
        source_name: locale.tr("search-source-commands").into(),
        source_icon: "commands".into(),
        title: locale.tr(&desc.display_name_key).into(),
        subtitle,
        shortcut: shortcut.unwrap_or_default().into(),
    }
}
