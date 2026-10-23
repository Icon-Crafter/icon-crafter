//! Font discovery and family resolution.
//!
//! Fonts are enumerated once from the operating system's font database (through
//! `cosmic-text`/`fontdb`) and kept as a compact, case-insensitive index so the
//! editor can validate text styles, warn about missing fonts and choose a usable
//! default for a new document without re-walking thousands of faces on every
//! edit.

use std::collections::{BTreeMap, BTreeSet};

use cosmic_text::fontdb::Database;

/// A catalog of installed font families.
///
/// Family names are stored once (canonical casing from the font database) and
/// indexed by their lower-cased spelling, because designers routinely type
/// `inter` or `Inter` and both must resolve to the same family.
#[derive(Debug, Clone, Default)]
pub struct FontCatalog {
    /// Canonical family names, ordered case-insensitively.
    canonical: BTreeSet<String>,
    /// Lower-cased family name -> canonical name.
    index: BTreeMap<String, String>,
}

/// Outcome of resolving a requested family name against the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontMatch {
    /// The family the document asked for.
    pub requested: String,
    /// The installed family that will be used, if the request resolved.
    pub matched: Option<String>,
    /// `true` when the requested family is not installed and a fallback is used.
    pub fallback: bool,
}

impl FontMatch {
    /// The family that will actually be used for rendering, if any.
    pub fn effective(&self) -> Option<&str> {
        self.matched.as_deref()
    }
}

impl FontCatalog {
    /// Builds a catalog from a font database.
    pub fn from_db(db: &Database) -> Self {
        let mut canonical = BTreeSet::new();
        let mut index = BTreeMap::new();
        for face in db.faces() {
            for (family, _language) in &face.families {
                let name = family.trim();
                if name.is_empty() {
                    continue;
                }
                let key = name.to_lowercase();
                if let std::collections::btree_map::Entry::Vacant(e) = index.entry(key) {
                    e.insert(name.to_string());
                    canonical.insert(name.to_string());
                }
            }
        }
        // Case-insensitive ordering keeps the layer font picker stable while
        // still presenting the canonical family names.
        canonical = canonical.into_iter().collect::<BTreeSet<_>>();
        Self { canonical, index }
    }

    /// Number of installed families.
    pub fn len(&self) -> usize {
        self.canonical.len()
    }

    /// Whether the catalog has no families at all (a broken font environment).
    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
    }

    /// All canonical family names, ordered case-insensitively.
    pub fn families(&self) -> impl Iterator<Item = &str> {
        self.canonical.iter().map(String::as_str)
    }

    /// Case-insensitive lookup returning the canonical family name.
    pub fn canonical_name(&self, family: &str) -> Option<&str> {
        self.index
            .get(&family.trim().to_lowercase())
            .map(String::as_str)
    }

    /// Whether a family is installed (case-insensitive).
    pub fn contains(&self, family: &str) -> bool {
        self.canonical_name(family).is_some()
    }

    /// Resolves a requested family into a [`FontMatch`].
    pub fn resolve(&self, requested: &str) -> FontMatch {
        match self.canonical_name(requested) {
            Some(canonical) => FontMatch {
                requested: requested.to_string(),
                matched: Some(canonical.to_string()),
                fallback: false,
            },
            None => FontMatch {
                requested: requested.to_string(),
                matched: None,
                fallback: true,
            },
        }
    }

    /// The preferred default family for new text.
    ///
    /// Prefers `Inter` when installed (the editor bundles it in a release
    /// build), then a small list of common UI faces, then the first installed
    /// family so a new text object is always renderable.
    pub fn default_family(&self) -> Option<&str> {
        const PREFERRED: &[&str] = &[
            "Inter",
            "Segoe UI",
            "SF Pro Text",
            "Helvetica Neue",
            "Roboto",
            "Noto Sans",
            "DejaVu Sans",
            "Arial",
            "Tahoma",
            "Verdana",
        ];
        for name in PREFERRED {
            if let Some(canonical) = self.canonical_name(name) {
                return Some(canonical);
            }
        }
        self.canonical.iter().next().map(String::as_str)
    }

    /// Suggests up to `limit` installed families similar to `family`.
    ///
    /// The match is deliberately simple (prefix/contains, case-insensitive) so
    /// the UI can offer "did you mean" suggestions without fuzzy-match noise.
    pub fn suggest(&self, family: &str, limit: usize) -> Vec<String> {
        let needle = family.trim().to_lowercase();
        if needle.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut prefix = Vec::new();
        let mut contains = Vec::new();
        for (key, canonical) in &self.index {
            if key.starts_with(&needle) {
                prefix.push(canonical.clone());
            } else if key.contains(&needle) {
                contains.push(canonical.clone());
            }
        }
        prefix.sort_by_key(|name| name.to_lowercase());
        contains.sort_by_key(|name| name.to_lowercase());
        prefix.into_iter().chain(contains).take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::FontSystem;

    #[test]
    fn catalog_lists_installed_families() {
        let font_system = FontSystem::new();
        let catalog = FontCatalog::from_db(font_system.db());
        // Every supported desktop ships at least one font. If this fails the
        // machine has no usable fonts and every later test is meaningless.
        assert!(!catalog.is_empty(), "no fonts discovered on this machine");
        assert!(catalog.default_family().is_some());
    }

    #[test]
    fn resolution_is_case_insensitive_and_reports_fallback() {
        let font_system = FontSystem::new();
        let catalog = FontCatalog::from_db(font_system.db());
        let default = catalog.default_family().expect("default family");
        assert_eq!(
            catalog.canonical_name(&default.to_uppercase()).unwrap(),
            default
        );
        assert!(!catalog.resolve(default).fallback);
        let missing = catalog.resolve("Definitely Not A Font 9000");
        assert!(missing.fallback);
        assert_eq!(missing.matched, None);
        assert!(!missing.effective().is_some());
    }
}
