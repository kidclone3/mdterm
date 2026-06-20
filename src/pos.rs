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

use std::collections::HashMap;

use flate2::read::GzDecoder;
use serde::Serialize;

const WEIGHTS_GZ: &[u8] = include_bytes!("../pos_model/weights.json.gz");
const CLASSES_TXT: &str = include_str!("../pos_model/classes.txt");
const TAGS_JSON: &str = include_str!("../pos_model/tags.json");

struct AveragedPerceptron {
    feature_weights: HashMap<String, HashMap<String, f32>>,
    classes: Vec<String>,
}

impl AveragedPerceptron {
    fn from_embedded() -> Self {
        let decoder = GzDecoder::new(WEIGHTS_GZ);
        let parsed: HashMap<String, HashMap<String, f64>> =
            serde_json::from_reader(decoder).expect("parse weights.json.gz");
        let feature_weights: HashMap<String, HashMap<String, f32>> = parsed
            .into_iter()
            .map(|(feat, inner)| {
                (
                    feat,
                    inner.into_iter().map(|(tag, w)| (tag, w as f32)).collect(),
                )
            })
            .collect();

        let classes: Vec<String> = CLASSES_TXT
            .split('\n')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();

        Self {
            feature_weights,
            classes,
        }
    }

    fn predict(&self, features: &HashMap<String, usize>) -> (&str, f32) {
        let mut scores: HashMap<&str, f32> = HashMap::new();
        for (feature, value) in features {
            if let Some(weights) = self.feature_weights.get(feature.as_str())
                && *value != 0
            {
                for (label, weight) in weights {
                    let score = scores.entry(label.as_str()).or_insert(0.0);
                    *score += weight * (*value as f32);
                }
            }
        }
        let class = self
            .classes
            .iter()
            .map(|c| c.as_str())
            .max_by(|a, b| {
                scores
                    .get(a)
                    .unwrap_or(&0.0)
                    .partial_cmp(scores.get(b).unwrap_or(&0.0))
                    .unwrap()
            })
            .unwrap_or("");
        let exp_sum: f32 = scores.values().map(|v| v.exp()).sum();
        let conf = if exp_sum > 0.0 {
            (scores.get(class).copied().unwrap_or(0.0)).exp() / exp_sum
        } else {
            0.0
        };
        (class, conf)
    }
}

#[derive(Serialize)]
pub struct Tag {
    pub word: String,
    pub tag: String,
    pub conf: f32,
}

pub struct PosTagger {
    model: AveragedPerceptron,
    tags: HashMap<String, String>,
}

impl PosTagger {
    /// Parse the embedded gzip-JSON model. Call once and cache.
    pub fn load() -> Self {
        let tags: HashMap<String, String> =
            serde_json::from_str(TAGS_JSON).expect("parse tags.json");
        Self {
            model: AveragedPerceptron::from_embedded(),
            tags,
        }
    }

    /// Tag a whitespace-separated sentence.
    pub fn tag(&self, sentence: &str) -> Vec<Tag> {
        let tokens: Vec<&str> = sentence.split_whitespace().collect();
        self.assign_tags(tokens)
    }

    fn assign_tags(&self, tokens: Vec<&str>) -> Vec<Tag> {
        let mut prev = "-START-".to_string();
        let mut prev2 = "-START2-".to_string();
        let mut output: Vec<Tag> = Vec::with_capacity(tokens.len());

        let mut context: Vec<String> = Vec::with_capacity(tokens.len() + 4);
        context.push(prev.clone());
        context.push(prev2.clone());
        for tok in &tokens {
            let mapped = if tok.contains('\'') && !tok.starts_with('\'') {
                "!HYPHEN".to_string()
            } else if tok.parse::<usize>().is_ok() && tok.len() == 4 {
                "!YEAR".to_string()
            } else if !tok.is_empty() && tok.as_bytes()[0].is_ascii_digit() {
                "!DIGITS".to_string()
            } else {
                (*tok).to_string()
            };
            context.push(mapped);
        }
        context.push("-END-".to_string());
        context.push("-END2-".to_string());

        let ctx: Vec<&str> = context.iter().map(|s| s.as_str()).collect();

        for (i, token) in tokens.iter().enumerate() {
            let (tag, conf) = if let Some(known) = self.tags.get(*token) {
                (known.clone(), 1.0)
            } else {
                let feats = Self::get_features(i + 2, token, &ctx, &prev, &prev2);
                let (t, c) = self.model.predict(&feats);
                (t.to_string(), c)
            };
            output.push(Tag {
                word: (*token).to_string(),
                tag: tag.clone(),
                conf,
            });
            prev2 = prev;
            prev = tag;
        }
        output
    }

    #[allow(clippy::too_many_arguments)]
    fn get_features(
        i: usize,
        word: &str,
        context: &[&str],
        prev: &str,
        prev2: &str,
    ) -> HashMap<String, usize> {
        let mut f: HashMap<String, usize> = HashMap::new();
        f.insert("bias".to_string(), 1);

        let suffix = if word.chars().count() > 3 {
            let from = word.char_indices().nth_back(2).map(|(b, _)| b).unwrap_or(0);
            &word[from..]
        } else {
            ""
        };
        f.insert(format!("i suffix {suffix}"), 1);

        let pref1 = word
            .chars()
            .nth(1)
            .map(|c| c.to_string())
            .unwrap_or_default();
        f.insert(format!("i pref1 {pref1}"), 1);

        f.insert(format!("i-1 tag {prev}"), 1);
        f.insert(format!("i-2 tag {prev2}"), 1);
        f.insert(format!("i tag+i-2 tag {prev} {prev2}"), 1);
        f.insert(format!("i word {}", context[i]), 1);
        f.insert(format!("i-1 tag+i word {prev} {}", context[i]), 1);
        f.insert(format!("i-1 word {}", context[i - 1]), 1);
        f.insert(format!("i-2 word {}", context[i - 2]), 1);
        f.insert(format!("i+1 word {}", context[i + 1]), 1);
        f.insert(format!("i+2 word {}", context[i + 2]), 1);

        let next = context[i + 1];
        let next_suffix = if next.chars().count() > 3 {
            let from = next.char_indices().nth_back(2).map(|(b, _)| b).unwrap_or(0);
            &next[from..]
        } else {
            ""
        };
        f.insert(format!("i+1 suffix {next_suffix}"), 1);

        let prv = context[i - 1];
        let prv_suffix = if prv.chars().count() > 3 {
            let from = prv.char_indices().nth_back(2).map(|(b, _)| b).unwrap_or(0);
            &prv[from..]
        } else {
            ""
        };
        f.insert(format!("i-1 suffix {prv_suffix}"), 1);

        f
    }
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

    #[test]
    fn tagger_loads_from_embedded_model() {
        let _t = PosTagger::load();
    }

    #[test]
    fn tagger_tags_known_sentence() {
        let t = PosTagger::load();
        let tags = t.tag("the quick brown fox jumps over the lazy dog");
        let words: Vec<&str> = tags.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(
            words,
            vec![
                "the", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog"
            ]
        );
        let the_tag = tags.iter().find(|x| x.word == "the").unwrap();
        assert_eq!(the_tag.tag, "DT");
        let fox = tags.iter().find(|x| x.word == "fox").unwrap();
        assert!(
            fox.tag.starts_with("NN"),
            "fox should be a noun, got {}",
            fox.tag
        );
    }
}
