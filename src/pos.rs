//! Part-of-speech highlighting.
//!
//! Gated behind the `pos` Cargo feature. When enabled, `apply()` colors prose
//! word-spans by their part of speech using a vendored averaged-perceptron
//! tagger (ported from `postagger`) and NLTK's pretrained model.

// Public surface is forward-looking: each item is consumed by a later task
// (apply pass, config/CLI category parsing). Silence dead_code until wired up.
#![allow(dead_code)]

/// The 9 part-of-speech color categories mdterm distinguishes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PosCategory {
    Noun = 0,
    Verb = 1,
    Adjective = 2,
    Adverb = 3,
    Preposition = 4,
    Conjunction = 5,
    Determiner = 6,
    Pronoun = 7,
    Value = 8,
}

const CATEGORY_COUNT: usize = 9;

const ALL_CATEGORY_NAMES: [&str; CATEGORY_COUNT] = [
    "noun",
    "verb",
    "adjective",
    "adverb",
    "preposition",
    "conjunction",
    "determiner",
    "pronoun",
    "value",
];

impl PosCategory {
    fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "noun" => Some(Self::Noun),
            "verb" => Some(Self::Verb),
            "adjective" => Some(Self::Adjective),
            "adverb" => Some(Self::Adverb),
            "preposition" => Some(Self::Preposition),
            "conjunction" => Some(Self::Conjunction),
            "determiner" => Some(Self::Determiner),
            "pronoun" => Some(Self::Pronoun),
            "value" => Some(Self::Value),
            _ => None,
        }
    }

    fn bit(self) -> u16 {
        1u16 << (self as u16)
    }
}

/// Dependency-free bitmask over [`PosCategory`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PosCategorySet(u16);

impl PosCategorySet {
    pub fn all() -> Self {
        Self((1u16 << CATEGORY_COUNT) - 1)
    }

    pub fn empty() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, c: PosCategory) {
        self.0 |= c.bit();
    }

    pub fn contains(&self, c: PosCategory) -> bool {
        self.0 & c.bit() != 0
    }

    /// Parse a list of category names. `"all"` yields every category.
    /// Unknown names return an error listing the valid names.
    pub fn from_names(names: &[String]) -> Result<Self, String> {
        if names.iter().any(|n| n.trim().eq_ignore_ascii_case("all")) {
            return Ok(Self::all());
        }
        let mut set = Self::empty();
        for n in names {
            match PosCategory::from_name(n.trim()) {
                Some(c) => set.insert(c),
                None => {
                    return Err(format!(
                        "unknown POS category '{n}'. Valid: {}",
                        ALL_CATEGORY_NAMES.join(", ")
                    ));
                }
            }
        }
        Ok(set)
    }
}

/// Map a Penn-Treebank tag to a color category. Punctuation, interjections,
/// foreign words, list markers, and unknown tags return `None` (uncolored).
pub fn pt_tag_to_category(tag: &str) -> Option<PosCategory> {
    let c = match tag {
        "NN" | "NNS" | "NNP" | "NNPS" => PosCategory::Noun,
        "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ" | "MD" => PosCategory::Verb,
        "JJ" | "JJR" | "JJS" => PosCategory::Adjective,
        "RB" | "RBR" | "RBS" | "RP" => PosCategory::Adverb,
        "IN" | "TO" => PosCategory::Preposition,
        "CC" => PosCategory::Conjunction,
        "DT" | "PDT" | "EX" | "POS" | "WDT" => PosCategory::Determiner,
        "PRP" | "PRP$" | "WP" | "WP$" | "WRB" => PosCategory::Pronoun,
        "CD" => PosCategory::Value,
        _ => return None,
    };
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noun_tags_map_to_noun() {
        assert_eq!(pt_tag_to_category("NN"), Some(PosCategory::Noun));
        assert_eq!(pt_tag_to_category("NNS"), Some(PosCategory::Noun));
        assert_eq!(pt_tag_to_category("NNP"), Some(PosCategory::Noun));
        assert_eq!(pt_tag_to_category("NNPS"), Some(PosCategory::Noun));
    }

    #[test]
    fn verb_tags_include_modals() {
        assert_eq!(pt_tag_to_category("VB"), Some(PosCategory::Verb));
        assert_eq!(pt_tag_to_category("VBD"), Some(PosCategory::Verb));
        assert_eq!(pt_tag_to_category("VBG"), Some(PosCategory::Verb));
        assert_eq!(pt_tag_to_category("VBN"), Some(PosCategory::Verb));
        assert_eq!(pt_tag_to_category("VBP"), Some(PosCategory::Verb));
        assert_eq!(pt_tag_to_category("VBZ"), Some(PosCategory::Verb));
        assert_eq!(pt_tag_to_category("MD"), Some(PosCategory::Verb));
    }

    #[test]
    fn adjective_adverb_preposition_conjunction() {
        assert_eq!(pt_tag_to_category("JJ"), Some(PosCategory::Adjective));
        assert_eq!(pt_tag_to_category("JJR"), Some(PosCategory::Adjective));
        assert_eq!(pt_tag_to_category("JJS"), Some(PosCategory::Adjective));
        assert_eq!(pt_tag_to_category("RB"), Some(PosCategory::Adverb));
        assert_eq!(pt_tag_to_category("RBR"), Some(PosCategory::Adverb));
        assert_eq!(pt_tag_to_category("RBS"), Some(PosCategory::Adverb));
        assert_eq!(pt_tag_to_category("RP"), Some(PosCategory::Adverb));
        assert_eq!(pt_tag_to_category("IN"), Some(PosCategory::Preposition));
        assert_eq!(pt_tag_to_category("TO"), Some(PosCategory::Preposition));
        assert_eq!(pt_tag_to_category("CC"), Some(PosCategory::Conjunction));
    }

    #[test]
    fn determiner_pronoun_value() {
        assert_eq!(pt_tag_to_category("DT"), Some(PosCategory::Determiner));
        assert_eq!(pt_tag_to_category("PDT"), Some(PosCategory::Determiner));
        assert_eq!(pt_tag_to_category("EX"), Some(PosCategory::Determiner));
        assert_eq!(pt_tag_to_category("POS"), Some(PosCategory::Determiner));
        assert_eq!(pt_tag_to_category("PRP"), Some(PosCategory::Pronoun));
        assert_eq!(pt_tag_to_category("PRP$"), Some(PosCategory::Pronoun));
        assert_eq!(pt_tag_to_category("WP"), Some(PosCategory::Pronoun));
        assert_eq!(pt_tag_to_category("WP$"), Some(PosCategory::Pronoun));
        assert_eq!(pt_tag_to_category("CD"), Some(PosCategory::Value));
    }

    #[test]
    fn punctuation_and_unknown_map_to_none() {
        for tag in [
            ".", ",", ":", "``", "''", "#", "$", "SYM", "UH", "-NONE-", "LS", "FW",
        ] {
            assert_eq!(pt_tag_to_category(tag), None, "tag {tag} should be None");
        }
        assert_eq!(pt_tag_to_category("ZZZ"), None);
    }

    #[test]
    fn category_set_all_contains_every_category() {
        let s = PosCategorySet::all();
        assert!(s.contains(PosCategory::Noun));
        assert!(s.contains(PosCategory::Value));
    }

    #[test]
    fn category_set_from_names_subset() {
        let s = PosCategorySet::from_names(&["noun".to_string(), "verb".to_string()]).unwrap();
        assert!(s.contains(PosCategory::Noun));
        assert!(s.contains(PosCategory::Verb));
        assert!(!s.contains(PosCategory::Adjective));
    }

    #[test]
    fn category_set_unknown_name_errors() {
        let err = PosCategorySet::from_names(&["nown".to_string()]).unwrap_err();
        assert!(err.contains("noun"), "error should list valid names: {err}");
    }
}
