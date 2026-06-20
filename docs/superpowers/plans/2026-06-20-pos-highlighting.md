# Part-of-Speech Highlighting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional part-of-speech coloring to mdterm prose, gated behind a `pos` Cargo feature, powered by a vendored averaged-perceptron tagger (from `postagger`) and NLTK's pretrained model.

**Architecture:** A new feature-gated `src/pos.rs` module runs a coloring pass over `Vec<Line>` between `markdown::render_with` and `style::wrap_lines`. It tags each in-scope line's words, maps Penn-Treebank tags to 9 color categories, and sets span foregrounds from the theme. Activation is opt-in via a `P` key, `--pos` CLI flag, and `[pos]` config subtable; a category subset is selectable via CLI/config.

**Tech Stack:** Rust edition 2024, clap 4 (CLI), serde + toml (config), bincode (embedded model), pulldown-cmark (frontmatter sniffing context), crossterm (TUI). Vendored: `postagger` averaged-perceptron logic (Apache-2.0) + NLTK `averaged_perceptron_tagger` model (Apache-2.0).

## Global Constraints

- **Rust edition 2024** (rustc 1.85+). Match existing style: no comments unless asked, `cargo fmt` clean.
- **Default build must remain functional and approximately the same binary size.** The `pos` feature is default-off. Additive core struct fields (`Style.code`, `DocumentInfo.frontmatter_lines`, 9 `Theme.pos_*`) are always-compiled but are a handful of bytes.
- **Every `Style {` literal in the codebase uses `..Default::default()`** (verified), so adding `Style.code: bool` is zero-churn. Do not touch unrelated construction sites.
- **License compatibility:** mdterm is MIT. Vendored code/data is Apache-2.0. Preserve original copyright notices; add a third-party credits section to README.
- **Category names** (lowercase, canonical): `noun verb adjective adverb preposition conjunction determiner pronoun value`. Unknown names must error with the accepted list.
- **Capital `P`** (Shift+P) is the toggle key, matching mdterm's capital-letter toggle convention (`t`, `L`).
- **Verification commands:** `cargo fmt --check`, `cargo check`, `cargo check --features pos`, `cargo build --features pos`, `cargo test`, `cargo test --features pos`.
- **Work in the worktree** at `.worktrees/pos-highlighting` on branch `feat/pos-highlighting`.

---

## File Structure

**Create:**
- `src/pos.rs` — feature-gated (`#[cfg(feature = "pos")]`) module: vendored perceptron, `PosCategory` enum + `PosCategorySet` bitmask, Penn-Treebank mapping, name parsing, `PosTagger`, tokenizer, `apply()` coloring pass.
- `pos_model/classes.txt` — vendored Penn-Treebank tag list (text).
- `pos_model/tags.json` — vendored word→tag exception dictionary.
- `pos_model/weights.bincode` — vendored perceptron weights, bincode-serialized (~2 MB).
- `examples/convert_pos_model.rs` — one-off tool: reads a `weights.json` path, writes `pos_model/weights.bincode`.

**Modify:**
- `Cargo.toml` — `pos` feature + optional `bincode` dependency; `[[example]]` for the converter (auto-detected, no entry needed).
- `src/main.rs` — `mod pos;` (cfg-gated), `--pos` CLI flag, resolution, piped-path wiring, install-hint when feature off.
- `src/style.rs` — `Style.code: bool`, `DocumentInfo.frontmatter_lines: Option<usize>`.
- `src/markdown.rs` — inline-code `Style.code = true`; frontmatter sniff in `render_with` → `doc_info.frontmatter_lines`.
- `src/theme.rs` — 9 `pos_*` color fields in struct + `dark()` + `light()`.
- `src/config.rs` — `[pos]` subtable (`PosConfig`).
- `src/viewer.rs` — pos state on `ViewerState`, `P` key handler, `pos::apply` call in `rebuild()`, status-bar hint, `ViewerOptions` fields.
- `.gitignore` — ignore `pos_model/weights.json` (the raw 5.7 MB source; only the bincode is committed).
- `README.md` — feature docs + third-party credits.

---

## Task 1: Core struct fields (`Style.code`, `DocumentInfo.frontmatter_lines`)

Always-compiled additive fields that later feature-gated code consumes. No behavior change yet.

**Files:**
- Modify: `src/style.rs:7-17` (Style struct), `src/style.rs:87-90` (DocumentInfo struct)
- Modify: `src/markdown.rs:1623-1659` (DocumentInfo construction in `render_with`)
- Modify: `src/json.rs:28`, `src/json.rs:929`, `src/json.rs:1814`, `src/json.rs:1860`, `src/json.rs:2216` (DocumentInfo constructions)
- Modify: `src/viewer.rs:439-441` (DocumentInfo construction in `ViewerState::new`)
- Test: `src/style.rs` (unit tests in existing `mod tests`)

**Interfaces:**
- Produces: `Style { ..., code: bool }` (default `false` via `#[derive(Default)]`); `DocumentInfo { code_blocks: Vec<CodeBlockContent>, frontmatter_lines: Option<usize> }`. Downstream tasks read `span.style.code` and `doc_info.frontmatter_lines`.

- [ ] **Step 1: Write failing tests in `src/style.rs`**

Add inside the existing `#[cfg(test)] mod tests` block (after the last test, before the closing brace):

```rust
    #[test]
    fn default_style_has_code_false() {
        let s = Style::default();
        assert!(!s.code);
    }

    #[test]
    fn document_info_default_frontmatter_is_none() {
        let di = DocumentInfo {
            code_blocks: Vec::new(),
            frontmatter_lines: None,
        };
        assert!(di.frontmatter_lines.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib style::tests`
Expected: FAIL — `code` and `frontmatter_lines` do not exist (compile error: no field `code` / no field `frontmatter_lines`).

- [ ] **Step 3: Add fields to the structs in `src/style.rs`**

Edit the `Style` struct (around line 8-17) — add `code: bool` after `link_url`:

```rust
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
    pub link_url: Option<String>,
    pub code: bool,
}
```

Edit the `DocumentInfo` struct (around line 88) — add `frontmatter_lines`:

```rust
pub struct DocumentInfo {
    pub code_blocks: Vec<CodeBlockContent>,
    pub frontmatter_lines: Option<usize>,
}
```

- [ ] **Step 4: Update all `DocumentInfo { ... }` literals to include the new field**

There are 7 construction sites. Each currently looks like `DocumentInfo { code_blocks: ... }`. Add `frontmatter_lines: None` to all except `src/markdown.rs` (set in Task 13; use `None` for now).

For `src/markdown.rs:1623` (inside `render_with`):
```rust
    let doc_info = DocumentInfo {
        code_blocks: renderer.code_blocks,
        frontmatter_lines: None,
    };
```

For each of these sites, change `DocumentInfo { code_blocks: <expr> }` to `DocumentInfo { code_blocks: <expr>, frontmatter_lines: None }`:
- `src/viewer.rs:439`
- `src/json.rs:28`
- `src/json.rs:929`
- `src/json.rs:1814`
- `src/json.rs:1860`
- `src/json.rs:2216`

- [ ] **Step 5: Run full test suite to verify everything compiles and passes**

Run: `cargo test`
Expected: PASS — all 168 existing tests still green, plus the 2 new tests.

- [ ] **Step 6: Commit**

```bash
git add src/style.rs src/markdown.rs src/json.rs src/viewer.rs
git commit -m "feat(pos): add Style.code and DocumentInfo.frontmatter_lines fields"
```

---

## Task 2: Theme POS color fields

Add 9 POS category color fields to `Theme`, populated in both `dark()` and `light()`.

**Files:**
- Modify: `src/theme.rs:80-93` (struct fields, insert before `is_dark`), `src/theme.rs:97-382` (`dark()`), `src/theme.rs:383-680` (`light()`)
- Test: `src/theme.rs` (add a `tests` module)

**Interfaces:**
- Produces: `Theme.pos_noun`, `.pos_verb`, `.pos_adjective`, `.pos_adverb`, `.pos_preposition`, `.pos_conjunction`, `.pos_determiner`, `.pos_pronoun`, `.pos_value` (all `Color`). Consumed by `pos::apply` in Task 8.

- [ ] **Step 1: Write failing test in `src/theme.rs`**

Add a tests module at the end of the file (if none exists):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_has_all_pos_fields() {
        let t = Theme::dark();
        let _ = (
            t.pos_noun, t.pos_verb, t.pos_adjective, t.pos_adverb,
            t.pos_preposition, t.pos_conjunction, t.pos_determiner,
            t.pos_pronoun, t.pos_value,
        );
    }

    #[test]
    fn light_theme_has_all_pos_fields() {
        let t = Theme::light();
        let _ = (
            t.pos_noun, t.pos_verb, t.pos_adjective, t.pos_adverb,
            t.pos_preposition, t.pos_conjunction, t.pos_determiner,
            t.pos_pronoun, t.pos_value,
        );
    }

    #[test]
    fn pos_colors_are_distinct_within_a_theme() {
        let t = Theme::dark();
        let colors = vec![
            t.pos_noun, t.pos_verb, t.pos_adjective, t.pos_adverb,
            t.pos_preposition, t.pos_conjunction, t.pos_determiner,
            t.pos_pronoun, t.pos_value,
        ];
        let mut sorted = colors.clone();
        sorted.sort_by_key(color_key);
        sorted.dedup();
        assert_eq!(sorted.len(), colors.len(), "POS colors should all differ");
    }

    fn color_key(c: &Color) -> String {
        match c {
            Color::Rgb { r, g, b } => format!("{r},{g},{b}"),
            other => format!("{other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib theme::tests`
Expected: FAIL — no field `pos_noun` on `Theme`.

- [ ] **Step 3: Add the 9 fields to the `Theme` struct**

In `src/theme.rs`, insert before the `is_dark: bool,` field (around line 92):

```rust
    // Part-of-speech highlighting (feature `pos`)
    pub pos_noun: Color,
    pub pos_verb: Color,
    pub pos_adjective: Color,
    pub pos_adverb: Color,
    pub pos_preposition: Color,
    pub pos_conjunction: Color,
    pub pos_determiner: Color,
    pub pos_pronoun: Color,
    pub pos_value: Color,

    is_dark: bool,
```

- [ ] **Step 4: Populate fields in `dark()`**

In the `dark()` constructor body (the `Self { is_dark: true, ... }` block ending before line 382), add the 9 fields with these muted values tuned for dark backgrounds. Insert anywhere inside the `Self { ... }` block (e.g., after `json_focus_bg`):

```rust
            pos_noun: Color::Rgb { r: 152, g: 204, b: 168 },   // sage green
            pos_verb: Color::Rgb { r: 134, g: 175, b: 255 },   // soft blue
            pos_adjective: Color::Rgb { r: 221, g: 175, b: 230 }, // lavender
            pos_adverb: Color::Rgb { r: 214, g: 188, b: 153 }, // wheat
            pos_preposition: Color::Rgb { r: 143, g: 197, b: 215 }, // teal
            pos_conjunction: Color::Rgb { r: 200, g: 162, b: 200 }, // mauve
            pos_determiner: Color::Rgb { r: 150, g: 155, b: 168 }, // cool gray
            pos_pronoun: Color::Rgb { r: 230, g: 180, b: 140 }, // peach
            pos_value: Color::Rgb { r: 209, g: 154, b: 102 },  // bronze
```

- [ ] **Step 5: Populate fields in `light()`**

In the `light()` constructor body (ending before line 680), add the same 9 fields with values tuned for light backgrounds:

```rust
            pos_noun: Color::Rgb { r: 56, g: 120, b: 80 },     // forest green
            pos_verb: Color::Rgb { r: 40, g: 90, b: 190 },     // deep blue
            pos_adjective: Color::Rgb { r: 140, g: 60, b: 160 }, // purple
            pos_adverb: Color::Rgb { r: 150, g: 110, b: 30 },  // dark wheat
            pos_preposition: Color::Rgb { r: 30, g: 120, b: 140 }, // deep teal
            pos_conjunction: Color::Rgb { r: 130, g: 60, b: 130 }, // plum
            pos_determiner: Color::Rgb { r: 95, g: 100, b: 115 }, // slate
            pos_pronoun: Color::Rgb { r: 180, g: 95, b: 40 },  // rust
            pos_value: Color::Rgb { r: 165, g: 110, b: 30 },   // amber
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib theme::tests`
Expected: PASS (3 tests). Then run `cargo build` to confirm the whole crate compiles.

- [ ] **Step 7: Commit**

```bash
git add src/theme.rs
git commit -m "feat(pos): add 9 POS category color fields to Theme (dark + light)"
```

---

## Task 3: Cargo `pos` feature scaffold + pos.rs stub

Wire up the feature flag and an empty feature-gated module so both build configurations work.

**Files:**
- Modify: `Cargo.toml`
- Create: `src/pos.rs`
- Modify: `src/main.rs:1-9` (module declarations)

**Interfaces:**
- Produces: `pos` Cargo feature (default off); `bincode` as optional dependency; `src/pos.rs` gated by `#[cfg(feature = "pos")]`. All later `pos::` items build on this.

- [ ] **Step 1: Add the feature and optional dependency in `Cargo.toml`**

After the `[dependencies]` table (end of file), append:

```toml
[features]
pos = ["dep:bincode"]

[optional-dependencies]
# (none yet; bincode declared below)
```

Note: Cargo's `[features] = ["dep:bincode"]` syntax requires `bincode` declared as an optional dependency in `[dependencies]`. Add `bincode` to the `[dependencies]` table as optional:

```toml
bincode = { version = "1", optional = true }
```

(Place it among the other `[dependencies]` entries, e.g., after `serde_json`.) Remove the `[optional-dependencies]` placeholder above — it was only illustrative; the correct mechanism is `optional = true` on the `[dependencies]` entry plus `dep:bincode` in `[features]`.

Final additions to `Cargo.toml`:
- In `[dependencies]`: `bincode = { version = "1", optional = true }`
- New section at end:
```toml
[features]
pos = ["dep:bincode"]
```

- [ ] **Step 2: Create `src/pos.rs` as an empty gated module**

```rust
//! Part-of-speech highlighting.
//!
//! Gated behind the `pos` Cargo feature. When enabled, `apply()` colors prose
//! word-spans by their part of speech using a vendored averaged-perceptron
//! tagger (ported from `postagger`) and NLTK's pretrained model.

// All module contents are added in later tasks.
```

- [ ] **Step 3: Register the module (feature-gated) in `src/main.rs`**

In the module declaration block (lines 1-9), add after `mod markdown;`:

```rust
#[cfg(feature = "pos")]
mod pos;
```

- [ ] **Step 4: Verify both configurations compile**

Run: `cargo build` (no feature)
Expected: PASS — builds without `pos` module.

Run: `cargo build --features pos`
Expected: PASS — empty `pos` module compiles, `bincode` pulled in.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/pos.rs src/main.rs
git commit -m "feat(pos): scaffold pos Cargo feature and gated module"
```

---

## Task 4: POS categories, Penn-Treebank mapping, and name parsing

Define the 9-category enum, a bitmask set, the PT-tag→category map, and the category-name parser used by config/CLI.

**Files:**
- Modify: `src/pos.rs`
- Test: `src/pos.rs` (add `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `enum PosCategory` (9 variants), `struct PosCategorySet(u16)` bitmask with `all()`, `from_names(&[String]) -> Result<Self,String>`, `contains(PosCategory) -> bool`; `fn pt_tag_to_category(&str) -> Option<PosCategory>`. Consumed by Tasks 6, 8, 12.

- [ ] **Step 1: Write failing tests in `src/pos.rs`**

Append to `src/pos.rs`:

```rust
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
        for tag in [".", ",", ":", "``", "''", "#", "$", "SYM", "UH", "-NONE-", "LS", "FW"] {
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
        let s = PosCategorySet::from_names(
            &["noun".to_string(), "verb".to_string()]
        ).unwrap();
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features pos --lib pos`
Expected: FAIL — `PosCategory`, `pt_tag_to_category`, `PosCategorySet` undefined.

- [ ] **Step 3: Implement categories, mapping, and the bitmask set**

Insert at the top of `src/pos.rs` (below the module doc comment):

```rust
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
    "noun", "verb", "adjective", "adverb", "preposition",
    "conjunction", "determiner", "pronoun", "value",
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
```

Note: `WRB` (wh-adverb like "where/when") is folded into `Pronoun` here so question words get a distinct color; `WDT` (wh-determiner) into `Determiner`. This matches the design's "QuestionWord folds into Pronoun/Adverb" note — `WRB` is treated as a Pronoun for coloring purposes since it reads as a query word.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features pos --lib pos`
Expected: PASS — 8 tests.

- [ ] **Step 5: Commit**

```bash
git add src/pos.rs
git commit -m "feat(pos): add PosCategory enum, bitmask set, and PT-tag mapping"
```

---

## Task 5: Vendor model files and conversion example

Download NLTK's perceptron model, commit the small text files + the bincode-converted weights, and add a regeneration tool. Requires network access to download the source weights once.

**Files:**
- Create: `pos_model/classes.txt`, `pos_model/tags.json`, `pos_model/weights.bincode`
- Create: `examples/convert_pos_model.rs`
- Modify: `.gitignore`
- Modify: `Cargo.toml` (the `bincode` optional dep from Task 3 is reused; no change needed, but confirm)

**Interfaces:**
- Produces: committed model data loadable via `include_bytes!("../pos_model/weights.bincode")`, `include_str!("../pos_model/classes.txt")`, `include_str!("../pos_model/tags.json")`. Consumed by Task 6.

- [ ] **Step 1: Download the source model files**

Run from the worktree root:

```bash
mkdir -p pos_model
curl -fsSL -o pos_model/classes.txt https://raw.githubusercontent.com/shubham0204/postagger.rs/main/tagger/classes.txt
curl -fsSL -o pos_model/tags.json https://raw.githubusercontent.com/shubham0204/postagger.rs/main/tagger/tags.json
curl -fsSL -o pos_model/weights.json https://raw.githubusercontent.com/shubham0204/postagger.rs/main/tagger/weights.json
```

Verify sizes: `classes.txt` ~193 bytes, `tags.json` ~25 KB, `weights.json` ~5.7 MB.

- [ ] **Step 2: Add `pos_model/weights.json` to `.gitignore`**

Append to `.gitignore`:

```
pos_model/weights.json
```

(The 5.7 MB JSON source is not committed; only the converted bincode is.)

- [ ] **Step 3: Write the conversion example**

Create `examples/convert_pos_model.rs`:

```rust
//! One-off tool: convert NLTK's `weights.json` into `pos_model/weights.bincode`.
//!
//! Usage:
//!     cargo run --example convert_pos_model -- pos_model/weights.json pos_model/weights.bincode
//!
//! Run this only when updating the vendored model. The output bincode is what
//! `src/pos.rs` embeds at compile time via `include_bytes!`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: convert_pos_model <weights.json> <weights.bincode>");
        process::exit(2);
    }
    let src = &args[1];
    let dst = &args[2];

    let text = fs::read_to_string(src).expect("read weights.json");
    let parsed: HashMap<String, HashMap<String, f64>> =
        serde_json::from_str(&text).expect("parse weights.json");

    // Downcast f64 -> f32 to halve size; the perceptron tolerates the precision loss.
    let weights: HashMap<String, HashMap<String, f32>> = parsed
        .into_iter()
        .map(|(feat, inner)| {
            (
                feat,
                inner.into_iter().map(|(tag, w)| (tag, w as f32)).collect(),
            )
        })
        .collect();

    let bytes = bincode::serialize(&weights).expect("serialize bincode");
    fs::write(dst, &bytes).expect("write weights.bincode");
    eprintln!("wrote {} ({} bytes) from {}", dst, bytes.len(), src);
}
```

This example uses `serde_json` and `bincode`. `serde_json` is already a dependency; `bincode` is optional under the `pos` feature. Examples can use optional deps when the feature is enabled, so build the example with `--features pos`.

- [ ] **Step 4: Generate the bincode weights**

Run:

```bash
cargo run --example convert_pos_model --features pos -- pos_model/weights.json pos_model/weights.bincode
```

Expected: prints `wrote pos_model/weights.bincode (<~3000000 bytes) from ...`. Verify the file exists and is roughly 2-3 MB.

- [ ] **Step 5: Clean up the source JSON**

```bash
rm pos_model/weights.json
```

(It is now gitignored anyway, but remove it so it is not accidentally staged.)

- [ ] **Step 6: Verify the model files are staged correctly**

```bash
git add pos_model/classes.txt pos_model/tags.json pos_model/weights.bincode examples/convert_pos_model.rs .gitignore
git status --short
```

Expected: 4 new files + `.gitignore` modified; `weights.json` must NOT appear.

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(pos): vendor NLTK perceptron model (classes/tags/bincode) + converter"
```

---

## Task 6: PosTagger — vendored perceptron loading the embedded model

Port `postagger`'s `AveragedPerceptron` + `PerceptronTagger`, adapted to load from embedded bytes/strings instead of file paths.

**Files:**
- Modify: `src/pos.rs`
- Test: `src/pos.rs` (extend `tests` module)

**Interfaces:**
- Produces: `struct PosTagger`, `impl PosTagger { pub fn load() -> Self; pub fn tag(&self, sentence: &str) -> Vec<Tag>; }`, `struct Tag { pub word: String, pub tag: String, pub conf: f32 }`. Consumed by Task 8.

- [ ] **Step 1: Write failing tests in `src/pos.rs`**

Add to the `tests` module:

```rust
    #[test]
    fn tagger_loads_from_embedded_model() {
        let _t = PosTagger::load();
    }

    #[test]
    fn tagger_tags_known_sentence() {
        let t = PosTagger::load();
        let tags = t.tag("the quick brown fox jumps over the lazy dog");
        let words: Vec<&str> = tags.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(words, vec!["the", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog"]);
        // "the" is in the exception dictionary as a determiner.
        let the_tag = tags.iter().find(|x| x.word == "the").unwrap();
        assert_eq!(the_tag.tag, "DT");
        // "fox" should be tagged as a noun (NN or NNP etc.)
        let fox = tags.iter().find(|x| x.word == "fox").unwrap();
        assert!(fox.tag.starts_with("NN"), "fox should be a noun, got {}", fox.tag);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features pos --lib pos`
Expected: FAIL — `PosTagger` undefined.

- [ ] **Step 3: Implement the vendored perceptron**

Append to `src/pos.rs` (this is a faithful adaptation of `postagger`'s `perceptron_tagger.rs`, changed to load from embedded data):

```rust
use std::collections::HashMap;

use serde::Serialize;

const WEIGHTS_BINCODE: &[u8] = include_bytes!("../pos_model/weights.bincode");
const CLASSES_TXT: &str = include_str!("../pos_model/classes.txt");
const TAGS_JSON: &str = include_str!("../pos_model/tags.json");

struct AveragedPerceptron {
    feature_weights: HashMap<String, HashMap<String, f32>>,
    classes: Vec<String>,
}

impl AveragedPerceptron {
    fn from_embedded() -> Self {
        let feature_weights: HashMap<String, HashMap<String, f32>> =
            bincode::deserialize(WEIGHTS_BINCODE).expect("deserialize weights.bincode");

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
    /// Deserialize the embedded bincode model. Call once and cache.
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

        let pref1 = word.chars().nth(1).map(|c| c.to_string()).unwrap_or_default();
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features pos --lib pos`
Expected: PASS — tagger loads and tags the sample sentence correctly.

- [ ] **Step 5: Commit**

```bash
git add src/pos.rs
git commit -m "feat(pos): vendored averaged-perceptron PosTagger loading embedded model"
```

---

## Task 7: Tokenizer with span-offset mapping

Split a line's spans into word tokens while tracking which span and byte offset each token came from, so tags can be mapped back onto spans.

**Files:**
- Modify: `src/pos.rs`
- Test: `src/pos.rs`

**Interfaces:**
- Produces: `struct Token { span_idx: usize, byte_start: usize, byte_len: usize, text: String }`, `fn tokenize_spans(spans: &[StyledSpan]) -> Vec<Token>`. Consumed by Task 8.

- [ ] **Step 1: Write failing tests in `src/pos.rs`**

Add to `tests`:

```rust
    use crate::style::{Style, StyledSpan};

    fn span(text: &str) -> StyledSpan {
        StyledSpan { text: text.to_string(), style: Style::default() }
    }

    #[test]
    fn tokenize_single_span_offsets() {
        let toks = tokenize_spans(&[span("the quick fox")]);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0].text, "the");
        assert_eq!(&span_text("the quick fox")[toks[0].byte_start..toks[0].byte_start + toks[0].byte_len], "the");
        assert_eq!(toks[2].text, "fox");
        assert_eq!(&span_text("the quick fox")[toks[2].byte_start..toks[2].byte_end()], "fox");
    }

    #[test]
    fn tokenize_across_spans_uses_correct_span_idx() {
        // span0: "hello " span1: "world!"
        let toks = tokenize_spans(&[span("hello "), span("world!")]);
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].span_idx, 0);
        assert_eq!(toks[0].text, "hello");
        assert_eq!(toks[1].span_idx, 1);
        assert_eq!(toks[1].text, "world!");
        // byte offsets are relative to each token's own span
        assert_eq!(&"world!"[toks[1].byte_start..toks[1].byte_end()], "world!");
    }

    #[test]
    fn tokenize_skips_pure_whitespace_runs() {
        let toks = tokenize_spans(&[span("   ")]);
        assert!(toks.is_empty());
    }

    fn span_text(s: &str) -> String { s.to_string() }

    // helper used above
    impl Token {
        fn byte_end(&self) -> usize { self.byte_start + self.byte_len }
    }
```

Note: `byte_end()` is added as a test-only helper via impl above; the real struct uses `byte_start + byte_len`. If the impl block in tests conflicts, instead inline `toks[0].byte_start + toks[0].byte_len` in the assertions and drop the helper impl. Prefer inlining to keep the struct surface minimal.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features pos --lib pos`
Expected: FAIL — `Token`, `tokenize_spans` undefined.

- [ ] **Step 3: Implement the tokenizer**

Append to `src/pos.rs` (note the `use` for `StyledSpan`):

```rust
use crate::style::StyledSpan;

/// A word token located within a specific span of a line.
#[derive(Clone, Debug)]
pub struct Token {
    /// Index into the line's `spans` vector.
    pub span_idx: usize,
    /// Byte offset where the token starts within `spans[span_idx].text`.
    pub byte_start: usize,
    /// Byte length of the token text.
    pub byte_len: usize,
    /// The token text itself (no surrounding whitespace).
    pub text: String,
}

/// Split a line's spans into non-whitespace word tokens, recording each
/// token's originating span and byte offset. Tokenization matches the
/// perceptron's `split_whitespace` expectation; punctuation stays attached
/// to the adjacent word.
pub fn tokenize_spans(spans: &[StyledSpan]) -> Vec<Token> {
    let mut out = Vec::new();
    for (idx, span) in spans.iter().enumerate() {
        let bytes = span.text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // skip whitespace
            while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let start = i;
            while i < bytes.len() && !(bytes[i] as char).is_whitespace() {
                i += 1;
            }
            let end = i;
            out.push(Token {
                span_idx: idx,
                byte_start: start,
                byte_len: end - start,
                text: span.text[start..end].to_string(),
            });
        }
    }
    out
}
```

- [ ] **Step 4: Adjust tests to inline `byte_end`**

If you kept the test helper impl from Step 1, remove it and replace `toks[x].byte_end()` with `toks[x].byte_start + toks[x].byte_len` in the assertions, then re-run.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features pos --lib pos`
Expected: PASS — all tokenizer + category + tagger tests green.

- [ ] **Step 6: Commit**

```bash
git add src/pos.rs
git commit -m "feat(pos): span-aware tokenizer with byte-offset mapping"
```

---

## Task 8: The `pos::apply` coloring pass

The heart of the feature: walk each in-scope line, tag it, split word spans, and set foregrounds by category.

**Files:**
- Modify: `src/pos.rs`
- Test: `src/pos.rs`

**Interfaces:**
- Consumes: `Line`, `LineMeta`, `Style`, `StyledSpan`, `Theme` from `crate::style` / `crate::theme`; `PosTagger`, `PosCategory`, `PosCategorySet`, `pt_tag_to_category`, `tokenize_spans`.
- Produces: `pub fn apply(lines: &mut Vec<Line>, theme: &Theme, tagger: &PosTagger, categories: PosCategorySet, frontmatter_lines: Option<usize>)`. Consumed by Tasks 13, 14.

- [ ] **Step 1: Write failing tests in `src/pos.rs`**

Add a helper and tests to the `tests` module:

```rust
    use crate::style::{Line, LineMeta};
    use crate::theme::Theme;

    fn plain_line(text: &str) -> Line {
        Line {
            spans: vec![StyledSpan { text: text.to_string(), style: Style::default() }],
            meta: LineMeta::None,
        }
    }

    fn fg_of_first_word(line: &Line) -> Option<crossterm::style::Color> {
        // first non-empty span's fg
        line.spans.iter().find(|s| !s.text.trim().is_empty()).and_then(|s| s.style.fg)
    }

    #[test]
    fn apply_colors_noun_and_verb_differently() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let mut lines = vec![plain_line("the fox runs quickly")];
        apply(&mut lines, &theme, &tagger, PosCategorySet::all(), None);
        // At least two distinct foregrounds appear among the word spans.
        let fgs: std::collections::HashSet<_> = lines[0]
            .spans.iter()
            .filter(|s| !s.text.trim().is_empty())
            .map(|s| format!("{:?}", s.style.fg))
            .collect();
        assert!(fgs.len() >= 2, "expected multiple POS colors, got {fgs:?}");
    }

    #[test]
    fn apply_preserves_bold_attribute() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let mut lines = vec![Line {
            spans: vec![StyledSpan {
                text: "the fox runs".to_string(),
                style: Style { bold: true, ..Style::default() },
            }],
            meta: LineMeta::None,
        }];
        apply(&mut lines, &theme, &tagger, PosCategorySet::all(), None);
        assert!(lines[0].spans.iter().all(|s| s.style.bold), "bold must survive apply");
    }

    #[test]
    fn apply_skips_inline_code_spans() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let code_color = theme.inline_code_fg;
        let mut lines = vec![Line {
            spans: vec![StyledSpan {
                text: "use foo".to_string(),
                style: Style { code: true, fg: Some(code_color), ..Style::default() },
            }],
            meta: LineMeta::None,
        }];
        apply(&mut lines, &theme, &tagger, PosCategorySet::all(), None);
        // code span keeps its original fg
        assert_eq!(fg_of_first_word(&lines[0]), Some(code_color));
    }

    #[test]
    fn apply_skips_link_spans() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let link_color = theme.link;
        let mut lines = vec![Line {
            spans: vec![StyledSpan {
                text: "click here".to_string(),
                style: Style {
                    fg: Some(link_color),
                    link_url: Some("http://x".to_string()),
                    ..Style::default()
                },
            }],
            meta: LineMeta::None,
        }];
        apply(&mut lines, &theme, &tagger, PosCategorySet::all(), None);
        assert_eq!(fg_of_first_word(&lines[0]), Some(link_color));
    }

    #[test]
    fn apply_skips_code_block_lines() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let before = "let x = 1;".to_string();
        let mut lines = vec![Line {
            spans: vec![StyledSpan { text: before.clone(), style: Style::default() }],
            meta: LineMeta::CodeContent { block_id: 0 },
        }];
        apply(&mut lines, &theme, &tagger, PosCategorySet::all(), None);
        // untouched: still one span with no fg
        assert_eq!(lines[0].spans.len(), 1);
        assert!(lines[0].spans[0].style.fg.is_none());
        assert_eq!(lines[0].spans[0].text, before);
    }

    #[test]
    fn apply_skips_frontmatter_lines() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let mut lines = vec![
            plain_line("title: Hello"),   // index 0 — frontmatter
            plain_line("the fox runs"),   // index 1 — real prose
        ];
        apply(&mut lines, &theme, &tagger, PosCategorySet::all(), Some(1));
        // line 0 untouched (no fg), line 1 colored
        assert!(lines[0].spans[0].style.fg.is_none());
        assert!(lines[1].spans.iter().any(|s| s.style.fg.is_some()));
    }

    #[test]
    fn apply_respects_category_subset() {
        let theme = Theme::dark();
        let tagger = PosTagger::load();
        let only_nouns = PosCategorySet::from_names(&["noun".to_string()]).unwrap();
        let mut lines = vec![plain_line("the fox runs quickly")];
        apply(&mut lines, &theme, &tagger, only_nouns, None);
        // Non-noun words keep no fg; at least the noun "fox" gets a color.
        let has_color = lines[0].spans.iter().any(|s| s.style.fg.is_some());
        assert!(has_color, "the noun should be colored");
        let noun_color = theme.pos_noun;
        let fox_colored = lines[0].spans.iter().any(|s| {
            s.text.contains("fox") && s.style.fg == Some(noun_color)
        });
        assert!(fox_colored, "'fox' should wear the noun color");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features pos --lib pos`
Expected: FAIL — `apply` undefined.

- [ ] **Step 3: Implement `apply`**

Append to `src/pos.rs`:

```rust
use crate::style::{Line, LineMeta};
use crate::theme::Theme;

fn category_color(theme: &Theme, cat: PosCategory) -> crossterm::style::Color {
    match cat {
        PosCategory::Noun => theme.pos_noun,
        PosCategory::Verb => theme.pos_verb,
        PosCategory::Adjective => theme.pos_adjective,
        PosCategory::Adverb => theme.pos_adverb,
        PosCategory::Preposition => theme.pos_preposition,
        PosCategory::Conjunction => theme.pos_conjunction,
        PosCategory::Determiner => theme.pos_determiner,
        PosCategory::Pronoun => theme.pos_pronoun,
        PosCategory::Value => theme.pos_value,
    }
}

/// Color prose word-spans by part of speech.
///
/// - Skips the first `frontmatter_lines` line indices.
/// - Skips lines whose `meta` is `CodeContent` or `DiagramContent`.
/// - Skips spans marked `style.code` (inline code) or `style.link_url` (links).
/// - Preserves all existing style attributes; only sets `fg`.
/// - Only colors words whose category is in `categories`.
pub fn apply(
    lines: &mut Vec<Line>,
    theme: &Theme,
    tagger: &PosTagger,
    categories: PosCategorySet,
    frontmatter_lines: Option<usize>,
) {
    let skip = frontmatter_lines.unwrap_or(0);
    for (line_idx, line) in lines.iter_mut().enumerate() {
        if line_idx < skip {
            continue;
        }
        if matches!(
            line.meta,
            LineMeta::CodeContent { .. } | LineMeta::DiagramContent { .. }
        ) {
            continue;
        }
        // Tokenize, but only non-exempt spans contribute to the sentence.
        let tokens = tokenize_spans(&line.spans);
        if tokens.is_empty() {
            continue;
        }
        // Build the sentence from tokens whose originating span is not exempt.
        let is_exempt = |span_idx: usize| -> bool {
            let s = &line.spans[span_idx];
            s.style.code || s.style.link_url.is_some()
        };
        let sentence: String = tokens
            .iter()
            .filter(|t| !is_exempt(t.span_idx))
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if sentence.is_empty() {
            continue;
        }
        let tagged = tagger.tag(&sentence);

        // Walk tokens in order, assigning each a tag from `tagged` in sequence.
        // Tokens from exempt spans don't consume a tag (they weren't in the sentence).
        let mut tag_iter = tagged.into_iter();
        // Collect recolored spans per span index, then splice back in.
        // Strategy: rebuild each span's contribution by splitting on its tokens.
        let mut new_spans: Vec<StyledSpan> = Vec::with_capacity(line.spans.len());
        // Precompute token indices grouped by span_idx.
        // (tokens are already in order; iterate spans and consume their tokens.)
        let mut tok_cursor = 0;
        let tokens_ref: &[Token] = &tokens; // borrow for indexing
        for (span_idx, span) in line.spans.iter().enumerate() {
            // Gather tokens belonging to this span.
            let mut start = 0usize;
            let text_len = span.text.len();
            let mut pieces: Vec<StyledSpan> = Vec::new();
            while tok_cursor < tokens_ref.len() && tokens_ref[tok_cursor].span_idx == span_idx {
                let tok = &tokens_ref[tok_cursor];
                // whitespace before the token
                if tok.byte_start > start {
                    pieces.push(StyledSpan {
                        text: span.text[start..tok.byte_start].to_string(),
                        style: span.style.clone(),
                    });
                }
                let tok_text = span.text[tok.byte_start..tok.byte_start + tok.byte_len].to_string();
                let exempt = is_exempt(span_idx);
                let fg = if exempt {
                    span.style.fg
                } else {
                    // consume a tag
                    match tag_iter.next() {
                        Some(t) => {
                            match pt_tag_to_category(&t.tag) {
                                Some(cat) if categories.contains(cat) => Some(category_color(theme, cat)),
                                _ => span.style.fg,
                            }
                        }
                        None => span.style.fg,
                    }
                };
                let mut style = span.style.clone();
                style.fg = fg;
                pieces.push(StyledSpan { text: tok_text, style });
                start = tok.byte_start + tok.byte_len;
                tok_cursor += 1;
            }
            // trailing whitespace/remainder
            if start < text_len {
                pieces.push(StyledSpan {
                    text: span.text[start..].to_string(),
                    style: span.style.clone(),
                });
            }
            if pieces.is_empty() {
                // span had no tokens (e.g., pure whitespace or empty) — keep as-is
                new_spans.push(span.clone());
            } else {
                new_spans.extend(pieces);
            }
        }
        line.spans = new_spans;
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features pos --lib pos`
Expected: PASS — all `pos` tests green.

- [ ] **Step 5: Run clippy on the feature build**

Run: `cargo clippy --features pos -- -D warnings`
Expected: no warnings. Fix any that surface (likely in the closure capturing `line.spans` — may need to restructure to avoid borrow-checker issues; if so, collect token ownership into a `Vec<Token>` owned copy before the span loop and iterate by value).

- [ ] **Step 6: Commit**

```bash
git add src/pos.rs
git commit -m "feat(pos): apply() coloring pass over prose lines"
```

---

## Task 9: Frontmatter detection in `render_with`

Sniff a leading YAML frontmatter block and record its line count so the POS pass can skip it.

**Files:**
- Modify: `src/markdown.rs:1623-1659` (`render_with`, set `frontmatter_lines`)
- Test: `src/markdown.rs` (extend `tests`)

**Interfaces:**
- Produces: `DocumentInfo.frontmatter_lines = Some(n)` for docs beginning with a `---\n...\n---\n` block; `None` otherwise. Consumed by `pos::apply` (Tasks 13, 14).

- [ ] **Step 1: Write failing tests in `src/markdown.rs`**

Add to the `tests` module:

```rust
    #[test]
    fn frontmatter_detected_for_leading_yaml_block() {
        let md = "---\ntitle: Hello\nauthor: Me\n---\n\n# Heading\n\nText.\n";
        let (_, info) = render_test(md);
        assert_eq!(info.frontmatter_lines, Some(4)); // 4 source lines: ---,title,author,---
    }

    #[test]
    fn no_frontmatter_for_plain_doc() {
        let md = "# Heading\n\nText.\n";
        let (_, info) = render_test(md);
        assert_eq!(info.frontmatter_lines, None);
    }

    #[test]
    fn no_frontmatter_when_fence_not_at_start() {
        let md = "# H\n\n---\n\ntext\n";
        let (_, info) = render_test(md);
        // thematic break mid-doc is not frontmatter
        assert_eq!(info.frontmatter_lines, None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib markdown::tests`
Expected: FAIL — `frontmatter_lines` is always `None`.

- [ ] **Step 3: Implement the sniffer and wire it in**

Add a helper function near `render_with` in `src/markdown.rs`:

```rust
/// Detect a leading YAML frontmatter block (`---\n ... \n---\n`) and return
/// the number of source lines it covers (inclusive of both fences).
fn frontmatter_line_count(input: &str) -> Option<usize> {
    let bytes = input.as_bytes();
    if !bytes.starts_with(b"---\n") && !bytes.starts_with(b"---\r\n") {
        return None;
    }
    // Search for a closing fence line that is exactly `---` (after the first fence).
    let mut line_no = 0usize;
    for line in input.lines() {
        line_no += 1;
        if line_no == 1 {
            continue; // opening fence
        }
        if line.trim_end() == "---" {
            return Some(line_no);
        }
    }
    None
}
```

Then in `render_with`, set the field (currently `frontmatter_lines: None` from Task 1):

```rust
    let doc_info = DocumentInfo {
        code_blocks: renderer.code_blocks,
        frontmatter_lines: frontmatter_line_count(input),
    };
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib markdown::tests`
Expected: PASS — the 3 new frontmatter tests pass and all existing markdown tests remain green.

- [ ] **Step 5: Commit**

```bash
git add src/markdown.rs
git commit -m "feat(pos): detect leading YAML frontmatter, record line count"
```

---

## Task 10: Mark inline-code spans with `Style.code`

Set `code: true` on the inline-code span so `pos::apply` exempts it.

**Files:**
- Modify: `src/markdown.rs:1075-1084` (the `Event::Code` handler's `tick_style` and `code_style`)

**Interfaces:**
- Consumes: `Style.code` (Task 1). Produces: inline-code spans carry `style.code == true`, read by `pos::apply`.

- [ ] **Step 1: Write a failing test in `src/markdown.rs`**

Add to `tests`:

```rust
    #[test]
    fn inline_code_span_marked_as_code() {
        let md = "Text with `code` inside.\n";
        let (lines, _) = render_test(md);
        // The line containing the code span has a span whose text is "code" and code==true.
        let found = lines.iter().any(|l| {
            l.spans.iter().any(|s| s.text == "code" && s.style.code)
        });
        assert!(found, "inline code span should have style.code == true");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib markdown::tests::inline_code_span_marked_as_code`
Expected: FAIL — no span has `code: true`.

- [ ] **Step 3: Set `code: true` on the inline-code style**

In `src/markdown.rs` around line 1080, change `code_style`:

```rust
                let code_style = Style {
                    fg: Some(self.theme.inline_code_fg),
                    bg: Some(self.theme.inline_code_bg),
                    code: true,
                    ..Default::default()
                };
```

Leave `tick_style` (the backtick glyphs) unchanged — they are not prose and `apply` only splits on tokens; ticks are 1-char punctuation spans that will simply pass through uncolored, which is fine.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib markdown::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/markdown.rs
git commit -m "feat(pos): mark inline-code spans with Style.code"
```

---

## Task 11: Config `[pos]` subtable

Add a `PosConfig` subtable to config, storing raw category strings. Resolution to `PosCategorySet` happens in feature-gated code.

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs` (add `tests` module)

**Interfaces:**
- Produces: `Config.pos: PosConfig` where `PosConfig { enabled: bool, categories: Option<Vec<String>> }`. Defaults: `enabled = false`, `categories = None` (meaning "all"). Consumed by Tasks 12, 13.

- [ ] **Step 1: Write failing tests in `src/config.rs`**

Add at end of file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pos_is_disabled_and_all_categories() {
        let c = Config::default();
        assert!(!c.pos.enabled);
        assert!(c.pos.categories.is_none());
    }

    #[test]
    fn parse_pos_enabled_with_categories() {
        let toml = r#"
[pos]
enabled = true
categories = ["noun", "verb"]
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.pos.enabled);
        assert_eq!(c.pos.categories.as_deref(), Some(&["noun".to_string(), "verb".to_string()][..]));
    }

    #[test]
    fn parse_pos_enabled_only_defaults_categories_none() {
        let toml = "[pos]\nenabled = true\n";
        let c: Config = toml::from_str(toml).unwrap();
        assert!(c.pos.enabled);
        assert!(c.pos.categories.is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib config::tests`
Expected: FAIL — no field `pos` on `Config`.

- [ ] **Step 3: Add `PosConfig` and the field**

In `src/config.rs`, add the struct and field:

```rust
#[derive(Deserialize, Clone, Debug)]
pub struct PosConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
}

impl Default for PosConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            categories: None,
        }
    }
}
```

Add the field to `Config` (with `#[serde(default)]`):

```rust
#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub line_numbers: bool,
    #[serde(default)]
    pub width: usize,
    #[serde(default)]
    pub pos: PosConfig,
}
```

Update `Config::default()`:

```rust
impl Default for Config {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            line_numbers: false,
            width: 0,
            pos: PosConfig::default(),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib config::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(pos): add [pos] config subtable (enabled + categories)"
```

---

## Task 12: CLI `--pos` flag + resolution + install hint

Add the `--pos` flag, resolve enabled/categories from CLI+config, and print the install hint when the feature is off.

**Files:**
- Modify: `src/main.rs` (Cli struct, main logic)
- Modify: `src/viewer.rs:29-37` (`ViewerOptions`, add pos fields) — declared here because main constructs ViewerOptions
- Test: `src/main.rs` (factor a pure resolution helper and unit-test it)

**Interfaces:**
- Produces: `Cli.pos: Option<Option<String>>` (None = not passed; Some(None) = `--pos`; Some(Some("noun,verb")) = `--pos noun,verb`); `ViewerOptions.pos_enabled: bool`, `ViewerOptions.pos_categories: Vec<String>` (raw, resolved later in the viewer). Also a feature-gated/ungated install-hint constant.

- [ ] **Step 1: Add a pure resolution helper and a failing test**

Add a helper module in `src/main.rs` (near the top, after the `Cli` struct) that is NOT feature-gated (so it can be tested without the feature):

```rust
mod pos_cli {
    /// Parsed `--pos` value: `None` (flag absent), `All` (`--pos` / `--pos all`),
    /// or `Some(names)` for an explicit list.
    pub enum PosArg {
        Absent,
        All,
        Some(Vec<String>),
    }

    impl PosArg {
        pub fn parse(raw: Option<&str>) -> Result<Self, String> {
            match raw {
                None => Ok(Self::All),
                Some(v) => {
                    let v = v.trim();
                    if v.eq_ignore_ascii_case("all") || v.is_empty() {
                        Ok(Self::All)
                    } else {
                        let names: Vec<String> = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if names.is_empty() {
                            Ok(Self::All)
                        } else {
                            Ok(Self::Some(names))
                        }
                    }
                }
            }
        }
    }

    pub const VALID_CATEGORIES: [&str; 9] = [
        "noun", "verb", "adjective", "adverb", "preposition",
        "conjunction", "determiner", "pronoun", "value",
    ];

    pub const INSTALL_HINT: &str =
        "POS highlighting requires: cargo install mdterm --features pos";
}
```

Add a test module at the end of `src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::pos_cli::PosArg;

    #[test]
    fn pos_arg_absent_is_all() {
        // `--pos` with no value -> All
        assert!(matches!(PosArg::parse(None), Ok(PosArg::All)));
    }

    #[test]
    fn pos_arg_explicit_all() {
        assert!(matches!(PosArg::parse(Some("all")), Ok(PosArg::All)));
        assert!(matches!(PosArg::parse(Some("ALL")), Ok(PosArg::All)));
        assert!(matches!(PosArg::parse(Some("")), Ok(PosArg::All)));
    }

    #[test]
    fn pos_arg_list() {
        match PosArg::parse(Some("noun,verb")) {
            Ok(PosArg::Some(v)) => assert_eq!(v, vec!["noun".to_string(), "verb".to_string()]),
            other => panic!("expected Some list, got {other:?}"),
        }
    }

    #[test]
    fn pos_arg_list_trims_and_drops_empties() {
        match PosArg::parse(Some(" noun , , verb ")) {
            Ok(PosArg::Some(v)) => assert_eq!(v, vec!["noun".to_string(), "verb".to_string()]),
            other => panic!("expected trimmed list, got {other:?}"),
        }
    }
}
```

Note: `PosArg` needs `Debug` derive for the panic messages. Add `#[derive(Debug)]` to it.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests`
Expected: FAIL — `pos_cli` module undefined.

- [ ] **Step 3: Add the `--pos` arg to `Cli`**

In `src/main.rs`, add to the `Cli` struct (after `no_color`):

```rust
    /// Part-of-speech highlighting (requires `pos` feature)
    #[arg(long, num_args = 0..=1, value_name = "CATEGORIES")]
    pos: Option<String>,
```

`num_args = 0..=1` makes `--pos` valid with or without a value. `cli.pos` is `Option<String>`: `None` when flag absent; `Some(s)` when present, where `s` may be empty (bare `--pos`) or the CSV list. (Clap with `num_args = 0..=1` and a trailing value: confirm clap represents bare `--pos` as `Some("")` or `None`; the `PosArg::parse` handles both. Verify the actual representation by running `mdterm --pos --help` and a manual invocation in Step 7; adjust `parse`'s input extraction accordingly.)

- [ ] **Step 4: Resolve and wire into `ViewerOptions`**

Add fields to `ViewerOptions` in `src/viewer.rs:29-37`:

```rust
pub struct ViewerOptions {
    pub files: Vec<String>,
    pub initial_content: String,
    pub filename: String,
    pub theme: Theme,
    pub slide_mode: bool,
    pub line_numbers: bool,
    pub width_override: Option<usize>,
    pub pos_enabled: bool,
    pub pos_categories: Vec<String>,
}
```

In `main()`, after reading config and CLI, resolve the pos settings. The install-hint path applies when the flag is passed but the feature is off:

```rust
    let pos_arg = match &cli.pos {
        None => None,
        Some(v) => Some(pos_cli::PosArg::parse(if v.is_empty() { None } else { Some(v.as_str()) })?),
    };

    #[cfg(not(feature = "pos"))]
    if pos_arg.is_some() {
        eprintln!("{}", pos_cli::INSTALL_HINT);
        process::exit(0);
    }

    // Resolve enabled + raw categories from CLI (overrides config).
    let (pos_enabled, pos_categories): (bool, Vec<String>) = match pos_arg {
        Some(pos_cli::PosArg::All) => (true, Vec::new()),
        Some(pos_cli::PosArg::Some(names)) => (true, names),
        Some(pos_cli::PosArg::Absent) => unreachable!(),
        None => (config.pos.enabled, config.pos.categories.clone().unwrap_or_default()),
    };
```

Then pass into the TUI branch's `ViewerOptions`:

```rust
            pos_enabled,
            pos_categories,
```

Add these two fields to the `ViewerOptions { ... }` literal in `main.rs` (around line 124-134).

Note: `config.pos` requires making `config.rs`'s `PosConfig` and `Config.pos` public (done in Task 11). Import `PosConfig` if needed.

- [ ] **Step 5: Handle the error return from `PosArg::parse`**

`PosArg::parse` returns `Result`. Since `pos_arg` is constructed in an expression that uses `?`, `main` must return a `Result` OR the `?` must be inside a closure. Simplest: change the parse call to match and handle:

```rust
    let pos_arg_parsed = match &cli.pos {
        None => Ok(None),
        Some(v) => pos_cli::PosArg::parse(if v.is_empty() { None } else { Some(v.as_str()) }).map(Some),
    };
    let pos_arg = match pos_arg_parsed {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            process::exit(2);
        }
    };
```

(Use this form rather than `?` since `main` returns `()`.)

- [ ] **Step 6: Verify both feature configurations build and tests pass**

Run: `cargo test --lib tests`
Expected: PASS (4 pos_cli tests).

Run: `cargo build`
Expected: PASS (ungated code; `--pos` parses, install-hint path compiles under `cfg(not(feature="pos"))`).

Run: `cargo build --features pos`
Expected: PASS.

- [ ] **Step 7: Manually verify the install hint (feature off)**

Run: `cargo run -- --pos README.md; echo "exit=$?"`
Expected: prints `POS highlighting requires: cargo install mdterm --features pos` and `exit=0`. (The viewer does NOT launch.)

- [ ] **Step 8: Commit**

```bash
git add src/main.rs src/viewer.rs
git commit -m "feat(pos): add --pos CLI flag, resolution, and install hint when feature off"
```

---

## Task 13: Viewer integration (state, `P` key, rebuild wiring, status bar)

Wire POS into the interactive viewer: state fields, lazy tagger load, `P` toggle, the `apply` call in `rebuild`, and the status-bar hint.

**Files:**
- Modify: `src/viewer.rs` (multiple sites)
- Test: `src/viewer.rs` (extend `tests` if present; otherwise manual verification)

**Interfaces:**
- Consumes: `pos::PosTagger`, `pos::apply`, `pos::PosCategorySet`, `ViewerOptions.pos_enabled/pos_categories`, `DocumentInfo.frontmatter_lines`.
- Produces: live POS toggle in the TUI.

- [ ] **Step 1: Add pos state fields to `ViewerState`**

In `src/viewer.rs` `ViewerState` struct (after `line_numbers: bool,` around line 304), add fields. The tagger + category set are feature-gated; `pos_enabled` is always present (harmlessly unused when feature off):

```rust
    // Options
    slide_mode: bool,
    line_numbers: bool,
    width_override: Option<usize>,
    pos_enabled: bool,
    #[cfg(feature = "pos")]
    pos_categories: crate::pos::PosCategorySet,
    #[cfg(feature = "pos")]
    pos_tagger: Option<crate::pos::PosTagger>,
```

- [ ] **Step 2: Initialize the fields in `ViewerState::new`**

In the `ViewerState { ... }` literal (around line 432-460), after `width_override: opts.width_override,`:

```rust
            width_override: opts.width_override,
            pos_enabled: opts.pos_enabled,
            #[cfg(feature = "pos")]
            pos_categories: {
                use crate::pos::PosCategorySet;
                if opts.pos_categories.is_empty() {
                    PosCategorySet::all()
                } else {
                    match PosCategorySet::from_names(&opts.pos_categories) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("{e}");
                            PosCategorySet::all()
                        }
                    }
                }
            },
            #[cfg(feature = "pos")]
            pos_tagger: None,
```

- [ ] **Step 3: Call `pos::apply` in `rebuild()` before wrapping**

In `rebuild()` (around line 574), change `let (lines, doc_info) =` to make `lines` mutable:

```rust
        let (mut lines, doc_info) = if self.filename.ends_with(".json") {
```

Then right before `self.wrapped = wrap_lines(&lines, cw);` (around line 657), insert:

```rust
        #[cfg(feature = "pos")]
        if self.pos_enabled && self.json_view.is_none() {
            if self.pos_tagger.is_none() {
                self.pos_tagger = Some(crate::pos::PosTagger::load());
            }
            if let Some(tagger) = &self.pos_tagger {
                crate::pos::apply(
                    &mut lines,
                    &self.theme,
                    tagger,
                    self.pos_categories,
                    doc_info.frontmatter_lines,
                );
            }
        }
        // Avoid unused-variable warning when feature is off.
        let _ = &mut lines;

        self.wrapped = wrap_lines(&lines, cw);
```

- [ ] **Step 4: Add the `P` key handler**

In the normal-mode key handler (around line 1730, after the `KeyCode::Char('L')` block), add:

```rust
        // Part-of-speech highlighting toggle
        KeyCode::Char('P') => {
            #[cfg(feature = "pos")]
            {
                state.pos_enabled = !state.pos_enabled;
                state.rebuild();
                state.set_toast(if state.pos_enabled {
                    "POS highlighting ON"
                } else {
                    "POS highlighting OFF"
                });
            }
            #[cfg(not(feature = "pos"))]
            {
                state.set_toast("POS highlighting requires: cargo install mdterm --features pos");
            }
        }
```

- [ ] **Step 5: Add the `P pos` hint to the status bar**

In `render_status_bar`, the normal-mode hint string (around line 3293):

```rust
    let hint = " h/l pan · / search · o toc · f links · L lines · ? help ";
```

Change to a feature-conditional hint:

```rust
    let hint = if cfg!(feature = "pos") {
        " h/l pan · / search · o toc · f links · L lines · P pos · ? help "
    } else {
        " h/l pan · / search · o toc · f links · L lines · ? help "
    };
```

`cfg!` (macro, not attribute) evaluates to a runtime bool so the same code compiles under both configurations.

- [ ] **Step 6: Add `P pos` to the slide-mode hint too**

The slide-mode hint at line 3116 is `" ←/→ navigate · t theme "`. Append the POS hint there as well using the same `cfg!` conditional, so slides also advertise the toggle:

```rust
        let hint = if cfg!(feature = "pos") {
            " ←/→ navigate · t theme · P pos "
        } else {
            " ←/→ navigate · t theme "
        };
```

- [ ] **Step 7: Verify both configurations build**

Run: `cargo build`
Expected: PASS (no feature; `P` shows the install-hint toast; `pos_enabled` field unused but harmless).

Run: `cargo build --features pos`
Expected: PASS.

Run: `cargo test`
Expected: PASS — all ungated tests still green.

Run: `cargo test --features pos`
Expected: PASS — all pos tests green.

- [ ] **Step 8: Manually verify live toggle**

Run: `cargo run --features pos -- README.md`
Then press `P`: prose words gain POS colors; press `P` again to turn off. Verify nouns/verbs differ in color, code blocks and inline `code` are uncolored, links keep their color. Quit with `q`.

- [ ] **Step 9: Commit**

```bash
git add src/viewer.rs
git commit -m "feat(pos): wire POS toggle (P key), lazy tagger, and rebuild pass into viewer"
```

---

## Task 14: Piped-output integration

Apply POS in the non-TTY (piped) path when `--pos` is passed.

**Files:**
- Modify: `src/main.rs:155-169` (piped branch)

**Interfaces:**
- Consumes: `pos::apply`, `PosTagger`, `PosCategorySet`, `DocumentInfo.frontmatter_lines`, resolved `pos_enabled`/`pos_categories` from Task 12.

- [ ] **Step 1: Resolve categories for the piped path**

In `main()`, the resolution block from Task 12 already computes `pos_enabled` and `pos_categories` (raw `Vec<String>`). For the feature-gated path we need a `PosCategorySet`. Compute it once (feature-gated) right after resolution:

```rust
    #[cfg(feature = "pos")]
    let pos_set = if pos_categories.is_empty() {
        pos::PosCategorySet::all()
    } else {
        match pos::PosCategorySet::from_names(&pos_categories) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                process::exit(2);
            }
        }
    };
```

(Place this near the top-level `let` bindings in `main`, guarded so it is only built when the feature is on. `pos::PosCategorySet::from_names` and `pos::PosCategorySet::all` exist from Task 4.)

- [ ] **Step 2: Apply POS before wrapping in the piped branch**

In `main.rs`, the piped branch currently does:

```rust
        let wrapped = style::wrap_lines(&lines, w);
```

Change to apply POS first when enabled (feature-gated):

```rust
        #[cfg(feature = "pos")]
        let mut lines = lines;
        #[cfg(feature = "pos")]
        if pos_enabled {
            let tagger = pos::PosTagger::load();
            pos::apply(&mut lines, &initial_theme, &tagger, pos_set, doc_info.frontmatter_lines);
        }
        let wrapped = style::wrap_lines(&lines, w);
```

Note: `lines` is currently bound immutably as `let (lines, _) = ...` in the piped branch (around line 156-163). Change that binding to `let (mut lines, doc_info) =` so the feature-gated block can mutate it; the `_` becomes `doc_info` because we now read `frontmatter_lines`. When the feature is off, `mut` is unused — silence with `let _ = &lines;` or build-gate the `mut`. Simplest: write `let (mut lines, doc_info) = ...;` and add `#[allow(unused_mut)]` is unnecessary because the feature-on build uses it; for the feature-off build, add a `let _ = &mut lines;` no-op guarded by `#[cfg(not(feature = "pos"))]`.

Concretely, replace the piped render+wrap block (lines ~147-169) with:

```rust
        let (mut lines, doc_info) = if is_json {
            match json::render(&content, w, &initial_theme) {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("JSON parse error: {}", e);
                    process::exit(1);
                }
            }
        } else {
            markdown::render(
                &content,
                w,
                &initial_theme,
                line_numbers,
                markdown::MermaidMode::AsciiThenSource,
            )
        };

        #[cfg(feature = "pos")]
        {
            if pos_enabled && !is_json {
                let tagger = pos::PosTagger::load();
                pos::apply(&mut lines, &initial_theme, &tagger, pos_set, doc_info.frontmatter_lines);
            }
        }
        #[cfg(not(feature = "pos"))]
        {
            let _ = &mut lines;
            let _ = &doc_info;
        }

        let wrapped = style::wrap_lines(&lines, w);
        if cli.no_color {
            viewer::print_lines_plain(&wrapped);
        } else {
            viewer::print_lines(&wrapped);
        }
```

- [ ] **Step 3: Verify both builds and run a manual pipe test**

Run: `cargo build`
Run: `cargo build --features pos`
Run: `cargo test`
Run: `cargo test --features pos`
Expected: all PASS.

Then:

```bash
cargo run --features pos -- --pos noun,verb README.md | head -20
```

Expected: prose nouns and verbs colored; piped to stdout (plain styled text). Confirm inline code and links are uncolored.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(pos): apply POS coloring in piped output when --pos is passed"
```

---

## Task 15: Documentation (README + third-party credits)

Document the feature, install options, config/CLI usage, and credits.

**Files:**
- Modify: `README.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Add a Features bullet and an install note**

In the Features list, add after the JSON viewer bullet:

```markdown
- **Part-of-speech highlighting** *(optional feature)* — Color prose words by part of speech (noun, verb, adjective, …) with a toggle, powered by a vendored averaged-perceptron tagger. Enable at install time; see below.
```

In the Installation section, add after the existing `cargo install --path .` block:

```markdown
### Optional: POS highlighting

The `pos` feature adds part-of-speech coloring (~2 MB of embedded model data). It
is off by default to keep the default build lean.

```bash
cargo install --features pos --path .    # with POS highlighting
cargo install --path .                   # without (default)
```
```

- [ ] **Step 2: Add a Controls entry and a Configuration example**

In the Features key table, add:

```markdown
| `P` | Toggle part-of-speech highlighting (requires `pos` feature) |
```

In the Configuration section, extend the example toml:

```markdown
```toml
theme = "dark"          # "dark" or "light"
line_numbers = false     # show line numbers in code blocks
width = 0               # display width (0 = auto)

[pos]                   # part-of-speech highlighting (requires `pos` feature)
enabled = false          # start with POS on
categories = ["noun", "verb"]  # only these; omit or "all" for every category
```
```

- [ ] **Step 3: Add a CLI reference entry**

In the CLI Reference options block, add:

```
    --pos [CATEGORIES]  Part-of-speech highlighting (e.g. --pos noun,verb; needs `pos` feature)
```

- [ ] **Step 4: Add third-party credits**

Add a new section before `## License`:

```markdown
## Third-Party Assets

The optional `pos` feature vendors:

- **Averaged-perceptron tagger logic** adapted from
  [`postagger.rs`](https://github.com/shubham0204/postagger.rs) (Apache-2.0).
- **Pretrained model** (`averaged_perceptron_tagger`) from
  [NLTK `nltk_data`](https://github.com/nltk/nltk_data) (Apache-2.0),
  redistributed in `pos_model/`.

Both are Apache-2.0 licensed and compatible with mdterm's MIT license.
```

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs(pos): document feature, install options, config, and credits"
```

---

## Task 16: Final verification

Full verification across both feature configurations.

**Files:** none (verification only; fix anything that surfaces).

- [ ] **Step 1: Format check**

Run: `cargo fmt`
Then: `git diff --exit-code` (if fmt changed anything, commit it).

If there are changes:

```bash
git add -A && git commit -m "style: cargo fmt"
```

- [ ] **Step 2: Clippy both configurations**

Run: `cargo clippy -- -D warnings`
Run: `cargo clippy --features pos -- -D warnings`
Expected: no warnings. Fix any that surface and commit.

- [ ] **Step 3: Full test suite both configurations**

Run: `cargo test`
Expected: PASS — all ungated tests green (the original 168 plus new ones from Tasks 1, 2, 9, 10, 11, 12).

Run: `cargo test --features pos`
Expected: PASS — everything above plus all `pos::tests` from Tasks 4, 6, 7, 8.

- [ ] **Step 4: Binary-size sanity check**

Run:
```bash
cargo build --release 2>/dev/null
ls -la target/release/mdterm | awk '{print $5" bytes (default)"}'
cargo build --release --features pos 2>/dev/null
ls -la target/release/mdterm | awk '{print $5" bytes (with pos)"}'
```

Expected: default build within a few KB of the pre-feature baseline; `--features pos` build larger by roughly the embedded bincode model (~2 MB) plus code.

- [ ] **Step 5: Manual smoke tests**

For each:
```bash
cargo run --features pos -- README.md        # then P to toggle, q to quit
cargo run --features pos -- --pos noun,verb README.md   # starts with noun+verb only
cargo run -- --pos README.md                 # prints install hint, exits 0
cargo run --features pos -- README.md | head # piped, no color unless --pos
cargo run --features pos -- --pos README.md | head  # piped, POS colored
echo '[pos]\nenabled = true\ncategories = ["verb"]' > /tmp/mdterm-cfg.toml
MDTERM_CONFIG=/tmp/mdterm-cfg.toml cargo run --features pos -- README.md  # (only if mdterm reads this env; otherwise edit ~/.config/mdterm/config.toml)
```

Verify: nouns/verbs/adjectives visually distinct; code blocks, inline code, links, and frontmatter uncolored; toggle toast appears; status bar shows `P pos` hint only in the feature build.

- [ ] **Step 6: Commit any verification fixes**

```bash
git status
# if anything changed:
git add -A && git commit -m "chore(pos): verification fixes"
```

---

## Self-Review

Run this checklist against the spec before handing off.

**Spec coverage (every spec requirement → a task):**
- Engine: vendor postagger + bincode → Tasks 5, 6.
- `pos` Cargo feature, default off → Task 3.
- Opt-in `P`/`--pos`/config; install hint → Tasks 11, 12, 13.
- Category subset via config + CLI → Tasks 4, 11, 12.
- Exempt code blocks + frontmatter → Tasks 8 (skip meta + frontmatter_lines), 9.
- Exempt inline code + links; preserve attrs → Tasks 8, 10.
- New `Style.code` flag → Tasks 1, 10.
- 9 theme fields dark+light → Task 2.
- Post-render pre-wrap pass at main.rs + viewer.rs → Tasks 13, 14.
- Frontmatter detection → Task 9.
- Lazy tagger load → Task 13 (first enable).
- Status bar indicator → Task 13.
- Piped output → Task 14.
- HTML export out of scope → no task (intentional).
- Licensing/attribution → Task 15.
- Verification matrix → Task 16.

**Placeholder scan:** No "TBD"/"TODO"/"handle edge cases" left. Each code step contains real code. The two spots flagged for implementer attention (clap's `--pos` representation in Task 12 Step 3/7; borrow-checker restructure in Task 8 Step 5) include concrete fallback instructions, not vague handwaves.

**Type consistency:** `PosCategorySet` API (`all`, `from_names`, `contains`, `insert`) is identical in Tasks 4, 8, 12, 13. `PosTagger::load`/`tag` consistent across 6, 8, 13, 14. `apply(...)` signature identical in 8, 13, 14. `DocumentInfo.frontmatter_lines: Option<usize>` consistent in 1, 8, 9, 13, 14. `Style.code: bool` consistent in 1, 8, 10. `ViewerOptions.pos_enabled`/`pos_categories` consistent in 12, 13.

**Known risk to monitor during execution:** Task 8's `apply()` has non-trivial borrow flow (iterating `line.spans` while building `new_spans`). The plan notes this in Step 5 with a concrete mitigation (collect tokens into an owned `Vec<Token>` first). If the borrow checker rejects the draft, restructure to own the tokens before the span loop. Do not weaken the test suite to work around it.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-20-pos-highlighting.md`. Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
