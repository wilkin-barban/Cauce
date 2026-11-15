//! Cauce translation catalogs for Spanish, English and Brazilian Portuguese.
//!
//! `S8` asks for the three languages from the first release, so `cauce-i18n` is a
//! data crate with no dependency at all: the GUI and the CLI both read the same
//! embedded tables and none of them owns a copy. The fallback chain is English and
//! then the key itself, so a gap shows up in the interface instead of panicking
//! (`docs/contracts.md` §2.6).
//!
//! Preset names and descriptions are not here: `docs/presets.md` ratifies three
//! columns that already travel inside `Preset`.

pub mod catalog;

pub use catalog::{Catalog, Locale, catalog, keys, missing_in, translate};
