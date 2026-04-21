# orchid-search

Full-text and metadata search for Orchid, powered by Tantivy. Exposes a single index per indexed root with pluggable analyzers and a schema that covers filenames, content, tags, and extracted metadata.

Index freshness is maintained by reacting to `notify` events streamed from `orchid-fs`, so changes appear in results without explicit re-scans.
