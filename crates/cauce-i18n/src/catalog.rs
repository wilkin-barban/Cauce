//! Locale, catalog and lookup rules for the three v0.1 languages (`S8`,
//! `docs/contracts.md` §2.6).
//!
//! A catalog is one flat `&[(&str, &str)]` sorted by key. The rules are the whole
//! contract, and none of them panics:
//!
//! 1. an exact hit wins;
//! 2. a miss falls back to English;
//! 3. a miss in English returns the key itself, so a gap is visible on screen
//!    instead of blanking the interface or aborting the process.
//!
//! Preset names and descriptions are deliberately absent: they are data of
//! `docs/presets.md` and live in the three ratified columns of `Preset`.

mod en;
mod es;
mod pt_br;

use std::sync::OnceLock;

/// The three languages of v0.1, in the order of `docs/contracts.md` §2.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Locale {
    Es,
    En,
    PtBr,
}

impl Locale {
    /// Every supported locale, in the documented order.
    pub const ALL: [Locale; 3] = [Locale::Es, Locale::En, Locale::PtBr];

    /// The exact wire tag of the settings contract (`es`, `en`, `pt-BR`).
    pub const fn tag(self) -> &'static str {
        match self {
            Locale::Es => "es",
            Locale::En => "en",
            Locale::PtBr => "pt-BR",
        }
    }

    /// Parses the contract tag; anything else, including `pt-br`, is not a locale.
    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|locale| locale.tag() == tag)
    }
}

/// One locale's table. Lookup is linear because the table is small and because
/// an ad-hoc catalog built by a caller is not required to be sorted.
pub struct Catalog {
    entries: &'static [(&'static str, &'static str)],
}

impl Catalog {
    pub const fn new(entries: &'static [(&'static str, &'static str)]) -> Self {
        Self { entries }
    }

    /// Every `(key, text)` pair, in the order the table declares them.
    pub fn entries(&self) -> &'static [(&'static str, &'static str)] {
        self.entries
    }

    /// The text of one key, or nothing when this catalog does not know it.
    pub fn get(&self, key: &str) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, text)| *text)
    }

    /// Exact hit, then `fallback`, then the key itself. Never a panic.
    pub fn resolve(&self, fallback: &Catalog, key: &'static str) -> &'static str {
        match self.get(key) {
            Some(text) => text,
            None => fallback.get(key).unwrap_or(key),
        }
    }
}

/// The catalog of one locale.
pub fn catalog(locale: Locale) -> &'static Catalog {
    static CATALOGS: [Catalog; 3] = [
        Catalog::new(es::ENTRIES),
        Catalog::new(en::ENTRIES),
        Catalog::new(pt_br::ENTRIES),
    ];
    match locale {
        Locale::Es => &CATALOGS[0],
        Locale::En => &CATALOGS[1],
        Locale::PtBr => &CATALOGS[2],
    }
}

/// The text of `key` in `locale`, falling back to English and then to the key.
pub fn translate(locale: Locale, key: &'static str) -> &'static str {
    catalog(locale).resolve(catalog(Locale::En), key)
}

/// Every key of the three catalogs, sorted and deduplicated: the parity budget
/// the tests hold the catalogs to.
pub fn keys() -> &'static [&'static str] {
    static KEYS: OnceLock<Vec<&'static str>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let mut keys: Vec<&'static str> = Locale::ALL
            .iter()
            .flat_map(|locale| catalog(*locale).entries().iter().map(|(key, _)| *key))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    })
    .as_slice()
}

/// The keys `locale` still owes. Empty means the catalog is complete.
pub fn missing_in(locale: Locale) -> Vec<&'static str> {
    keys()
        .iter()
        .copied()
        .filter(|key| catalog(locale).get(key).is_none())
        .collect()
}
