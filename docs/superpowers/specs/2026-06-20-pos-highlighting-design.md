# Part-of-Speech Highlighting Design

## Goal

Bring Compromise-style part-of-speech (POS) coloring to mdterm prose: nouns,
verbs, adjectives, and friends each get a distinct color, inspired by
[hi-pos.nvim](https://github.com/maxonvim/hi-pos.nvim). The feature must stay
opt-in at install and at runtime so mdterm's default experience and binary size
are unchanged.

## Background

hi-pos.nvim colors prose in Neovim using the JavaScript
[Compromise](https://github.com/spencermountain/compromise) NLP library. mdterm
is a pure-Rust, single-binary tool with no runtime dependencies, so we cannot
shell out to Node. A survey of the Rust ecosystem found one suitable existing
solution:

- **`postagger`** (crates.io, Apache-2.0): a ~250-line averaged-perceptron
  tagger porting NLTK's `averaged_perceptron_tagger`. Clean deps (serde,
  serde_json). Its crates.io "non-standard" license is a false alarm — the
  actual `LICENSE` is Apache-2.0; `Cargo.toml` just uses `license-file` instead
  of an SPDX string.
- Other crates were rejected: `viterbi_pos_tagger` (less proven model),
  `nlp`/`ultra-nlp` (unmaintained, no focused tagger), `rsmorphy`/`igo-rs`/
  `lindera-*`/`bareun`/`litsea` (non-English), `anno` (NER transformers,
  overkill).

Decision: **vendor `postagger`'s source and NLTK's pretrained model into
mdterm**, embedding the model as bincode. This reuses a proven algorithm and
model with no new runtime dependency.

## Scope

In scope:

- New optional Cargo feature `pos` (default off).
- Vendored averaged-perceptron POS tagger + NLTK model, embedded at compile
  time as bincode.
- POS coloring applied as a post-render, pre-wrap pass over `Vec<Line>`.
- Runtime opt-in via `P` key, `--pos` CLI flag, and `[pos]` config table.
- Selectable category subset via config and CLI.
- Exclusion of fenced/indented code blocks and frontmatter.
- Inline-code and link spans exempt from POS color; bold/italic/strikethrough
  attributes preserved.
- New `Style.code` flag to identify inline-code spans.
- POS color fields in both dark and light themes.
- Graceful "install hint" behavior when the feature is not compiled in.

Out of scope (defer to follow-up):

- HTML export of POS colors (`--export html`). The export path keeps current
  behavior. Piped and TUI output are the primary targets for this feature.
- Live TUI overlay for picking categories. Category selection is config + CLI
  only.
- A from-scratch Compromise-style rule-based tagger. The perceptron tagger is
  sufficient; a smaller rule-based engine is not worth the effort now.
- Re-tokenizing punctuation away from words. Punctuation stays attached to the
  preceding word and inherits its POS color, matching hi-pos visually.

## Architecture

mdterm's existing data flow is:

```
markdown text
  -> pulldown-cmark events
  -> Renderer (markdown.rs::render_with) -> (Vec<Line>, DocumentInfo)
  -> wrap_lines (style.rs)
  -> terminal / HTML output
```

The POS feature inserts one new pass between render and wrap:

```
  -> render_with -> (Vec<Line>, DocumentInfo)
  -> pos::apply(&mut lines, ...)      // NEW, feature-gated
  -> wrap_lines
  -> output
```

Applying POS color before wrapping is deliberate: `wrap_lines` already
preserves per-span styles, so coloring the unwrapped paragraph lines gives the
tagger better sentence context, tags fewer/longer lines, and the colors survive
wrapping for free. This mirrors how `wrap_lines` is already a decoupled
post-processing pass.

`pos::apply` is called at the two in-scope wrap sites when POS is enabled:

- `src/main.rs:164` (piped output)
- `src/viewer.rs:665` (interactive TUI)

`src/export.rs:18` is intentionally **not** modified in this feature. HTML
export does not map POS colors to CSS yet, so the pass is not wired there. A
follow-up can add it by inserting the same `pos::apply` call before
`export.rs`'s `wrap_lines`.

### Feature-gating boundary

Only the POS machinery is feature-gated: the `src/pos.rs` module, the embedded
`pos_model/` data, the `pos::apply` calls, and the `--pos`/`P`-key activation
logic that actually loads the model. A few additive fields live in core,
always-compiled structs so they do not need `#[cfg]` sprinkled across the
codebase:

- `Style.code: bool` (style.rs)
- `DocumentInfo.frontmatter_lines: Option<usize>` (style.rs)
- 9 `Theme.pos_*` color fields (theme.rs)

These add a handful of bytes to the default binary and no logic, so the
"default build size unchanged" claim holds to within negligible rounding.
Frontmatter detection runs always (it just records the range); only its
*use* by `pos::apply` is gated.

## Components

### 1. Vendored Tagger and Model

New directory `pos_model/` (committed to the repo):

- `classes.txt` (193 bytes) — Penn-Treebank tag list, one per line.
- `tags.json` (~25 KB) — word-to-tag exception dictionary.
- `weights.bincode` — the perceptron feature weights, converted once from
  NLTK's `weights.json` (5.7 MB) to bincode (~2 MB, ~10x faster to deserialize
  than JSON).

Source files come from NLTK's `averaged_perceptron_tagger.zip` via the
`postagger` repo; both are Apache-2.0.

New module `src/pos.rs`, gated with `#[cfg(feature = "pos")]`, contains:

- A vendored copy of `postagger`'s `AveragedPerceptron` and `PerceptronTagger`
  logic (~250 LOC), adapted to load from embedded `&[u8]` / `&str` instead of
  file paths:
  - `weights.bincode` via `include_bytes!("../pos_model/weights.bincode")`.
  - `classes.txt` and `tags.json` via `include_str!`.
  - The original loads JSON weights at runtime; we deserialize bincode once
    and cache. `tags.json` stays JSON (small enough that `include_str!` plus a
    one-shot serde_json parse is trivial, and it matches the vendored code's
    existing reader so less code changes).
- A lazily-initialized `PosTagger` handle.

Public surface:

```rust
pub struct PosTagger { /* vendored perceptron + exception dict */ }

impl PosTagger {
    /// Deserialize the embedded bincode model. Called once, cached by caller.
    pub fn load() -> Self;
    /// Tag a sentence; tokens are whitespace-split words. Returns word/tag/conf.
    pub fn tag(&self, sentence: &str) -> Vec<Tag>;
}
```

### 2. POS Categories and Penn-Treebank Mapping

Collapse the 45 Penn-Treebank tags to 9 color categories:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum PosCategory {
    Noun, Verb, Adjective, Adverb, Preposition,
    Conjunction, Determiner, Pronoun, Value,
}
```

Mapping:

| Category    | Penn-Treebank tags                 |
|-------------|------------------------------------|
| Noun        | NN, NNS, NNP, NNPS                 |
| Verb        | VB, VBD, VBG, VBN, VBP, VBZ, MD    |
| Adjective   | JJ, JJR, JJS                       |
| Adverb      | RB, RBR, RBS, RP                   |
| Preposition | IN, TO                             |
| Conjunction | CC                                 |
| Determiner  | DT, PDT, EX, POS                   |
| Pronoun     | PRP, PRP$, WP, WP$                 |
| Value       | CD                                 |

Punctuation tags (`.`, `,`, `:`, `` ` ``, `#`, `$`, `SYM`) and any unknown tag
map to `None` (no color applied). hi-pos's `Url`/`HashTag`/`AtMention` are not
needed here because mdterm already styles those as links/code; `QuestionWord`
folds into Pronoun (WP) / Adverb (WRB already in Adverb); `Expression`
interjections (UH) are rare and left uncolored.

### 3. Tokenization and Span Mapping

`pos::apply` walks each in-scope line and:

1. Concatenates the line's word-bearing span text into a sentence.
2. Splits into tokens, recording `(span_index, byte_offset_in_span, byte_len)`
   for each token so tags can be mapped back to source positions.
3. Calls `PosTagger::tag` on the sentence.
4. Walks the spans, splitting on word boundaries, and sets each word
   sub-span's `fg` to its category color when the category is in the active
   set.

Tokenization uses `split_whitespace` semantics to match the tagger's
expectations. Punctuation remains attached to the adjacent word and inherits
that word's color.

### 4. Exemptions and the `Style.code` Flag

Within colored regions, two span kinds are exempt from POS color:

- **Links** — already detectable via `Style.link_url.is_some()`.
- **Inline code** — needs a new marker.

Add a field to `Style` in `src/style.rs`:

```rust
pub struct Style {
    // ...existing fields...
    pub code: bool,   // true for inline-code spans
}
```

Default `false`. The inline-code emission in `src/markdown.rs` sets `code: true`
on those spans. `apply()` skips any span where `style.code` is true or
`style.link_url.is_some()`. For all other spans, `apply()` sets `fg` and
preserves `bold`/`italic`/`strikethrough`/`underline`/`dim`.

### 5. Frontmatter Exclusion

mdterm does not currently enable pulldown-cmark's frontmatter option, so a
leading `---\n...\n---\n` YAML block renders as a thematic break plus prose
lines. To exclude it without changing existing rendering:

- In `markdown.rs::render_with`, sniff the input for a leading frontmatter
  block (opening `---\n` at byte 0, closing `\n---\n`).
- Compute the count of source lines covered and store it as a new
  `DocumentInfo` field: `pub frontmatter_lines: Option<usize>`.
- `pos::apply` accepts `frontmatter_lines` and skips the first N rendered line
  indices.

This keeps frontmatter rendering byte-identical to today while letting the POS
pass skip it. (Code blocks and diagrams are already skipped via existing
`LineMeta::CodeContent` / `DiagramContent`.)

### 6. Theme Fields

Add 9 fields to `Theme` in `src/theme.rs`, with tuned values in both `dark()`
and `light()` constructors:

```rust
pub struct Theme {
    // ...existing fields...
    pub pos_noun: Color,
    pub pos_verb: Color,
    pub pos_adjective: Color,
    pub pos_adverb: Color,
    pub pos_preposition: Color,
    pub pos_conjunction: Color,
    pub pos_determiner: Color,
    pub pos_pronoun: Color,
    pub pos_value: Color,
}
```

Colors should be readable against the theme background and distinguishable from
existing semantic colors (headings, links, code). A muted, harmonious palette
is preferred over saturated rainbow.

### 7. Category Set and Activation Plumbing

Compile-time:

- `[features] pos = ["dep:bincode"]` in `Cargo.toml`.
- `bincode` added as an optional dependency.
- Default build (`cargo install mdterm`) excludes the feature; the 2 MB model
  is not embedded. Opt-in: `cargo install mdterm --features pos`.

Runtime state on `ViewerState`:

```rust
pos_enabled: bool,                       // toggle state
pos_categories: PosCategorySet,          // active subset (bitmask)
pos_tagger: Option<PosTagger>,           // lazily loaded on first enable
```

`PosCategorySet` is a small dependency-free bitmask defined in `src/pos.rs`
(e.g. a `pub struct PosCategorySet(u16);` with set/contains/insert helpers, or
a `bitflags!` macro). No new external crate is required for this.

Activation surfaces:

- **`P` key** (capital `P`, i.e. Shift+P — matching mdterm's existing
  capital-letter toggle convention for `t`/`L`) toggles `pos_enabled`. On first
  enable, `PosTagger::load()` deserializes the embedded bincode and caches the
  handle; subsequent toggles reuse it. Re-renders the document.
- **`--pos [LIST]`** CLI flag, `num_args(0..=1)`:
  - `--pos` with no value: POS on, categories from config (or all).
  - `--pos noun,verb`: POS on, only the listed categories.
  - `--pos all`: POS on, all categories.
- **`[pos]` config subtable** in `~/.config/mdterm/config.toml` (mdterm's
  first subtable; serde derives it):

  ```toml
  [pos]
  enabled = false                       # default off
  categories = ["noun", "verb"]         # omit or "all" -> all 9 highlighted
  ```

  Semantics: `categories` omitted or `"all"` -> all 9; a list -> only those;
  `enabled = false` -> off regardless.

Resolution order: CLI `--pos` overrides config; config overrides defaults;
`P` toggles the resolved state live without changing config.

Category names are lowercase canonical: `noun verb adjective adverb
preposition conjunction determiner pronoun value`. Unknown names on the CLI or
in config are rejected with an error listing the accepted names.

Feature-not-compiled behavior:

- `--pos` still parses (so `--help` is consistent) but prints
  `POS highlighting requires: cargo install mdterm --features pos` and exits.
- `P` key shows the same hint as a toast/status message.
- Config `[pos]` table is ignored with no error.

Status bar shows a `POS` indicator when active (alongside the existing
theme/line-number indicators).

### 8. Output Modes

- **TUI**: full toggle and category support.
- **Piped**: applies POS when `--pos` is passed, for consistent styled output
  (`mdterm --pos noun,verb doc.md | less -R`).
- **HTML export**: out of scope for this spec; export keeps current behavior.

## Data Flow

```
config.toml [pos] + CLI --pos + P key
  -> resolve (pos_enabled, pos_categories)
  -> markdown::render_with -> (Vec<Line>, DocumentInfo { frontmatter_lines, .. })
  -> if pos_enabled && cfg!(feature="pos"):
       PosTagger::load() (cached) -> pos::apply(&mut lines, theme, tagger,
                                                pos_categories, frontmatter_lines)
  -> style::wrap_lines(lines, width)
  -> viewer / piped output
```

`pos::apply` iterates lines, skips frontmatter (first N indices) and any line
whose `meta` is `CodeContent`/`DiagramContent`, splits each remaining line's
spans on word boundaries, tags the sentence, and sets `fg` on word sub-spans
whose category is in the active set — skipping spans with `style.code` or
`style.link_url`.

## Licensing and Attribution

- `postagger` source: Apache-2.0. Vendoring is permitted with attribution.
- NLTK model files: Apache-2.0 (from `nltk_data`).
- mdterm: MIT. Apache-2.0 is compatible.

Add a third-party credits section to the README noting both sources and
licenses. Keep the vendored `postagger` code's original copyright notice.

## Error Handling

- Tagging is best-effort and approximate by nature; it never errors out of the
  render pipeline. If `PosTagger::load()` fails (corrupt embedded model), log a
  warning and disable POS for the session rather than crashing.
- Unknown category names in CLI/config produce a clear error listing accepted
  names; the program does not silently ignore typos.
- Frontmatter sniffing is conservative: if the pattern is ambiguous, treat it
  as no frontmatter (color normally) rather than skipping real content.
- When the feature is not compiled in, `--pos` and `P` degrade to the install
  hint, never to a panic or unrecognized-flag error.

## Verification

Automated:

- `cargo fmt --check`
- `cargo check` (without feature)
- `cargo check --features pos`
- `cargo build --features pos`
- `cargo test` (without feature — the existing 168 tests must remain green;
  new non-gated tests for `Style.code`, `frontmatter_lines`, and theme fields
  may add to the count)
- `cargo test --features pos` (new tests included)

New unit tests (under `#[cfg(feature = "pos")]`):

- Penn-Treebank tag -> `PosCategory` mapping for representative tags and
  unknown/punctuation tags.
- Tokenizer offset mapping: a line with mixed plain/bold/code/link spans
  produces correct `(span_index, byte_offset, byte_len)` per token.
- `pos::apply` on fixture lines:
  - plain prose line gets word colors.
  - bold span keeps `bold` and gains POS `fg`.
  - inline-code span (`style.code = true`) is exempt.
  - link span (`link_url` set) is exempt.
  - `LineMeta::CodeContent` line is skipped entirely.
  - frontmatter lines (first N) are skipped.
  - category subset: only active categories get color; others pass through.

New integration test:

- Render a small fixture markdown with `--features pos`, enable POS, and assert
  that specific known words (e.g. "fox" -> Noun, "jumps" -> Verb) carry the
  expected category color, and that a fenced code block's contents are
  uncolored.

Manual:

- `cargo run --features pos -- --pos noun,verb README.md` shows only nouns and
  verbs colored.
- `P` toggles coloring live; status bar reflects state.
- `cargo run -- README.md` (no feature) with `--pos` prints the install hint.
- `cargo build` (no feature) binary size is unchanged from baseline.

Success criteria:

- Default build binary size unchanged.
- `--features pos` build adds roughly the bincode model size (~2 MB) plus code.
- POS coloring is visually clear and readable in both dark and light themes.
- Code blocks, frontmatter, inline code, and links are never POS-colored.
- All pre-existing tests continue to pass without the feature.
