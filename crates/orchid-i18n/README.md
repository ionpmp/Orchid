# orchid-i18n

Localization for Orchid. Uses ICU for locale-aware formatting, pluralisation, and collation; message catalogues are stored as TOML files under `locales/` at the workspace root.

The crate also decides RTL vs LTR layout direction at runtime so that the Slint UI can react without hard-coding locale rules.
