# Mermaid Native Renderer — Graph Family (Batch G) Design

## Goal

Render the four highest-leverage Mermaid diagram types as native ASCII without
any web image fallback: `stateDiagram` / `stateDiagram-v2`, `classDiagram`,
`erDiagram`, and `mindmap`. These reuse the existing `Canvas` infrastructure
and serve as the first batch of a larger program to replace the `mermaid.ink`
web-image pipeline entirely.

## Background

`src/diagram.rs` already renders two diagram types natively:

- `flowchart` / `graph` (TD and LR) — full `Canvas`, layer assignment,
  barycenter ordering, LR routing lanes.
- `sequenceDiagram` — participants, messages, notes, self-loops, blocks.

Every other diagram type currently falls back to a `mermaid.ink` PNG served
over HTTPS and rendered through the Kitty/iTerm2/Sixel image pipeline. That
approach is being retired. The long-term plan is one native renderer per
Mermaid type, split across four batches:

- **Batch G (this spec):** graph family — `state`, `class`, `er`, `mindmap`.
- **Batch R:** remove `mermaid.ink`, `MermaidMode::Image`, `AsciiThenImage`,
  and related image-pipeline wiring.
- **Batch C:** charts — `pie`, `gantt`, `xychart`, `quadrant`, `radar`.
- **Batch T:** time/narrative — `timeline`, `journey`, `gitGraph`.
- **Batch S:** specialized — `sankey`, `block`/`architecture`, C4 family,
  `packet`, `requirementDiagram`, `zenuml`, `kanban`.

Each batch gets its own spec and implementation plan.

## Scope

In scope:

- Splitting `src/diagram.rs` (2520 lines today) into a `src/diagram/` module
  tree to make room for new renderers.
- Native parsers and renderers for `stateDiagram` / `stateDiagram-v2`,
  `classDiagram` / `classDiagram-v2`, `erDiagram`, and `mindmap`.
- Extending `Canvas` and `draw_edge_*` with the minimal new primitives those
  renderers need (new node shapes, edge head/tail styles, crow's-foot
  endpoints, tree edges).
- New theme fields for member-visibility colors, key badges, and composite
  state backgrounds.

Out of scope:

- Removing `mermaid.ink` and `MermaidMode::Image` — Batch R.
- Chart primitives (axes, bars, wedges) — Batch C.
- Time-axis layouts — Batch T.
- `dot` / `graphviz` / `plantuml` rendering — explicitly excluded.
- Touching the JSON graph view's existing use of `Canvas`.
- Changing the public API `pub fn render_mermaid(code: &str, theme: &Theme)
  -> Option<(Vec<Vec<StyledSpan>>, usize)>`.

## Architecture

### Module decomposition

`src/diagram.rs` is replaced by a module tree. The split is a pure code move
first — all existing tests must pass unchanged — then new files are added.

```
src/diagram/
  mod.rs              public API: render_mermaid(), dispatch by keyword
  canvas.rs           Canvas, CanvasCell, draw_node, draw_node_with_height,
                      draw_card, draw_edge_td, draw_edge_lr, junction_char,
                      to_span_rows, conn constants   (~900 lines, extracted)
  theme.rs            edge_color(), palette tables   (~120 lines)
  graph/
    mod.rs            assign_layers, order_within_layers, refine_lr_layer_order,
                      NodeLayout, port maps, lane helpers (~600 lines, extracted)
    flowchart.rs      parse_mermaid() for graph/flowchart, render_td, render_lr,
                      Graph/Node/Edge/Direction types  (~600 lines, extracted)
    state.rs          NEW — parse_state_diagram(), render_state()
    class.rs          NEW — parse_class_diagram(), render_class()
    er.rs             NEW — parse_er_diagram(), render_er()
    mindmap.rs        NEW — parse_mindmap(), render_mindmap()
  sequence.rs         parse_sequence(), render_sequence() (~510 lines, extracted)
```

`Canvas`, `CardDrawRow`, `NodeShape`, `NodeLayout`, and the connection
constants become `pub(crate)` at the module root for cross-file reuse.

### Role of `Canvas`

`Canvas` is the 2D character buffer between logical drawing operations and
the `Vec<Vec<StyledSpan>>` the viewer renders. It provides three jobs:

1. Coordinate target — renderers say "draw a box centered at (5,3), width 11"
   without manually placing each `│`, `─`, `╮` glyph.
2. Edge merging — `set_edge` automatically composes the right junction char
   when edges cross (`│` + `─` → `┼`).
3. Style batching — `to_span_rows()` groups adjacent same-style cells into a
   single `StyledSpan`.

`Canvas` is preserved unchanged for Batch G. `classDiagram`/`erDiagram`/
`stateDiagram` reuse `draw_card`, `draw_node`, `draw_edge_td`, and
`draw_edge_lr` as-is. Only `mindmap` adds a new layout algorithm; it still
draws through `Canvas`.

### Dispatch

`render_mermaid` matches on the first diagram keyword. After Batch G, the
dispatch table is:

```rust
match first_diagram_keyword(code) {
    Some("sequenceDiagram") => sequence::render(code, theme),
    Some("stateDiagram") | Some("stateDiagram-v2") => graph::state::render(code, theme),
    Some("classDiagram") | Some("classDiagram-v2") => graph::class::render(code, theme),
    Some("erDiagram") => graph::er::render(code, theme),
    Some("mindmap") => graph::mindmap::render(code, theme),
    Some(kw) if is_unsupported_diagram(kw) => None,  // pending later batches
    _ => graph::flowchart::render(code, theme),       // graph TD/LR
}
```

`is_unsupported_diagram` loses its four `state`/`class`/`er`/`mindmap`
entries; the remaining ~20 entries are removed in Batches C/T/S.

## Components

### stateDiagram (-v2)

Closest cousin to flowchart. Reuses `Canvas`, `draw_node`, `assign_layers`,
and TD/LR routing. New work is mostly a parser.

**Parser (`graph/state.rs`):**

- Header: `stateDiagram` or `stateDiagram-v2`.
- Simple state: `Idle` or `state "Long label" as Idle`.
- Transition: `A --> B : label`. Reuses `parse_arrow` from flowchart.
- Pseudo-states: `[*] --> Idle`, `Idle --> [*]`.
- Composite states: `state Foo { … }` — recursive block parsing.
- Fork/Join: `state fork_state <<fork>>` / `<<join>>`.
- Note: `note left of X : text` (mirror sequence's note placements: `Over`,
  `LeftOf`, `RightOf`).
- Skipped directives: `skinparam`, `direction`, `scale`, `note right of`,
  styling lines.

**Internal representation:** reuses `Graph` with `direction: TopDown`. New
`NodeShape` variants:

- `Circle` (already exists) — initial state, rendered as `◉` inside a small
  circle.
- `Final` — new: outer ring + inner filled dot, drawn as 3x3
  `╭─╮ │◉│ ╰─╯`.
- `ForkBar` — new: 5–9 cell wide `═` bar, one row tall.

**Layout:** reuse `assign_layers` and `order_within_layers` unchanged.
Composite states are rendered recursively: parse the inner graph first,
render it into a sub-canvas, then wrap as a single tall node in the parent
layout. `draw_node_with_height` already supports arbitrary heights.

**Edges:** reuse `draw_edge_td` / `draw_edge_lr` with no changes.

**Estimated size:** ~300 lines.

### classDiagram (-v2)

UML class boxes with attributes, methods, and typed relationships. The card
rendering already exists as `Canvas::draw_card` (used by the JSON graph
view); this batch is mostly a parser plus new edge endpoints.

**Parser (`graph/class.rs`):**

- Header: `classDiagram` or `classDiagram-v2`.
- Bare class: `class Foo` — empty card with just the title.
- Class with body:
  ```
  class Foo {
    +publicAttr: Type
    -privateAttr: Type
    #protectedAttr: Type
    ~packageAttr: Type
    +method(): ReturnType
    -privateMethod(arg: Type)
  }
  ```
  Lines inside `{}` are collected verbatim into rows, preserving visibility
  markers (`+`/`-`/`#`/`~`) and types.
- Generic types: `class Foo<T, K>` — strip generics for layout width,
  render them in the title.
- Stereotype annotations: `<<interface>>`, `<<abstract>>`, `<<enum>>` —
  rendered as a row above the class name. May appear inside the class body
  as `<<interface>>` on its own line, or as `class Foo [<<interface>>]`.
- Relationships:
  - `<|--` inheritance, `--|>` reverse.
  - `*--` composition, `o--` aggregation.
  - `-->` association, `..>` dependency (dashed), `<|..` realization (dashed).
  - Cardinality: `A "1" --> "0..n" B : owns`.
  - Label after `:`.
- Skipped: `namespace`, `direction`, styling lines.

**Internal representation:** new `ClassNode { name, stereotype:
Option<String>, members: Vec<MemberRow> }`. Each `MemberRow { visibility:
char, text: String }`. New `ClassEdge { from, to, style: RelationshipStyle,
cardinalities: (Option<String>, Option<String>), label: Option<String> }`.

`RelationshipStyle` encodes the four properties independently:
`{ line: Solid|Dashed, head: None|Arrow|HollowArrow|FilledDiamond|
HollowDiamond, tail: same }`. The combination determines the rendered
shape; e.g. `<|--` is `{ Solid, HollowArrow at parent end, None }`.

**Layout:** TD or LR, honoring `direction` (default TD). Reuse
`assign_layers` + barycenter ordering. Each class rendered via `draw_card`
with:

- Optional stereotype row first.
- Title row = class name.
- One `CardDrawRow` per member, with the visibility char colored distinctly
  (`+` green, `-` red, `#` yellow, `~` blue).

**Edges:** `draw_edge_td` and `draw_edge_lr` are extended to take an
`EdgeStyle { dashed, head, tail, label, far_label }` instead of just a
label string. Existing flowchart/sequence call sites construct `EdgeStyle`
from their current parameters with `Arrow`/no tail to preserve rendering
exactly. Cardinality renders near each endpoint via the new `far_label`
slot; the main label stays in its existing mid-edge position.

**Estimated size:** ~450 lines.

### erDiagram

Entity-relationship diagrams for DB schemas. Structurally similar to
classDiagram (entities = cards with field lists) but edges carry
crow's-foot cardinality.

**Parser (`graph/er.rs`):**

- Header: `erDiagram`.
- Entity with body:
  ```
  entity CUSTOMER {
    bigint id PK
    string name
    timestamp created_at "created timestamp"
  }
  ```
  Each field: `type name [PK|FK] [comment]`. Comment is a quoted trailing
  string.
- Standalone entity without body: `entity CUSTOMER` (empty card).
- Relationship: `CUSTOMER ||--o{ ORDER : places`. Three tokens between the
  entity names: left cardinality (2 chars), joiner (`--` or `..`), right
  cardinality (2 chars). Label after `:`.

**Crow's-foot decoding:**

| Token | Meaning | Endpoint glyph |
|---|---|---|
| `\|\|` | exactly one | `│\|` |
| `\|o` | zero or one | `│o` |
| `}o` | zero or many | `⟨o` |
| `}\|` | one or many | `⟨\|` |
| `o\|` (mirrored) | zero or one (right side) | `o\|` |
| `o{` (mirrored) | zero or many (right side) | `o⟩` |
| `\|{` (mirrored) | one or many (right side) | `\|⟩` |

**Internal representation:** new `ErEntity { name, fields: Vec<ErField> }`
with `ErField { type_name, name, key: Option<KeyKind>, comment:
Option<String> }`. `KeyKind = Pk | Fk`. New `ErRel { from, from_card: Card,
to, to_card: Card, dashed: bool, label: Option<String> }`.

**Layout:** TD only (Mermaid's erDiagram has no LR mode). Reuse
`assign_layers` + barycenter ordering.

**Rendering:**

- Each entity via `draw_card`: title = entity name; one `CardDrawRow` per
  field. Field row: ` type  name  [PK/FK badge]` — `PK` badge yellow, `FK`
  badge blue.
- Crow's-foot endpoints drawn via a new helper
  `draw_crowsfoot(canvas, x, y, dir, card)` that lays down 2 chars at the
  endpoint before the connecting line begins. The trunk uses
  `draw_edge_td`, shortened by ~3 cells at each end to leave room for the
  crow's-foot decoration.

**Estimated size:** ~350 lines.

### mindmap

Tree radiating from a center root. Genuinely new layout, but draws through
`Canvas` and reuses `draw_node`.

**Parser (`graph/mindmap.rs`):**

- Header: `mindmap`.
- Strictly indentation-based hierarchy (no braces). Default indent is two
  spaces per level; the parser measures the indent of the first non-root
  line and treats multiples of that as depth.
  ```
  mindmap
    root((the root))
      Level 1 A
        Level 2 A
        Level 2 B
      Level 1 B
  ```
- Node shape from syntax, reusing `parse_node_ref` from flowchart:
  `root((label))` = circle, `id[label]` = rectangle, `id(label)` = rounded,
  `id{label}` = diamond.
- Skipped: `:::` style classes, `classDef` lines.

**Internal representation:** `MindNode { label, shape, children:
Vec<MindNode> }` — a plain recursive tree.

**Layout algorithm:**

1. Compute subtree height for each node = max(1, sum of children subtree
   heights).
2. Partition root's children into two halves. Children at even indices
   (0, 2, 4, …) go to the right half; children at odd indices (1, 3, 5, …)
   go to the left half. This deterministic alternation keeps the two
   halves balanced regardless of count.
3. Lay out the right half as a vertical stack of subtrees, with each
   subtree's root at column `RIGHT_COL_1`. Recurse: each child's children
   go to `RIGHT_COL_2`, etc.
4. Mirror for the left half.
5. Root sits at column 0, vertically centered between the two halves.

Invariant: every parent is vertically centered on the midpoint of its
children's y-range.

**Edges:** new helper `draw_tree_edge(canvas, parent_right_x, parent_cy,
child_left_x, child_cy)` — an orthogonal connector with a single rounded
bend, reusing the mid_x logic from `draw_edge_lr`. No arrowheads.

**Estimated size:** ~250 lines.

## Shared infrastructure additions

### New Canvas primitives (canvas.rs)

- `draw_crowsfoot(canvas, x, y, dir, card)` — crow's-foot endpoint for
  erDiagram.
- `draw_tree_edge(canvas, px, py, cx, cy)` — orthogonal tree connector for
  mindmap.
- New `NodeShape::Final` and `NodeShape::ForkBar` variants + handling in
  `draw_node` / `draw_node_with_height`.
- `EdgeStyle` struct replacing the `label: Option<&str>` parameter of
  `draw_edge_td` / `draw_edge_lr`:
  ```rust
  pub(crate) struct EdgeStyle<'a> {
      pub dashed: bool,
      pub head: EdgeEnd,
      pub tail: EdgeEnd,
      pub label: Option<&'a str>,
      pub far_label: Option<&'a str>,
  }
  pub(crate) enum EdgeEnd { None, Arrow, HollowArrow, FilledDiamond, HollowDiamond }
  ```
  Existing flowchart and sequence callers construct `EdgeStyle::default()`
  with `head: Arrow` (sequence) or `head: Arrow` plus the existing label
  (flowchart) — behavior is byte-identical to today.

### Theme additions (theme.rs)

- Reuse the existing `edge_color()` palette for all graph-family edges.
- New fields, with values for both dark and light themes:
  - `member_plus`, `member_minus`, `member_hash`, `member_tilde` —
    classDiagram visibility colors.
  - `key_badge_pk`, `key_badge_fk` — erDiagram key badge colors.
  - `composite_state_bg` — background tint for nested stateDiagram boxes.

## Error handling and dispatch precedence

**Hard requirement:** if a native renderer fails for any reason — parse
error, unsupported syntax, or a renderer bug — mdterm must (a) show the
user what went wrong, and (b) still render the original mermaid source as
a normal code block. A malformed diagram must never produce a blank gap,
a stale `mermaid.ink` image, or a panic that kills the TUI.

### Failure modes

Two failure modes are distinguished:

- **Parse failure.** The parser cannot make sense of the source
  (malformed mermaid, unsupported syntax within a type, empty body).
  Today this silently returns `None`; in Batch G it carries a reason.
- **Render failure.** The parser succeeded but the renderer panicked
  during layout or canvas drawing (a bug in the new code, or pathological
  input the layout algorithm doesn't handle). The dispatcher must catch
  these via `std::panic::catch_unwind` so one bad diagram cannot crash
  the whole TUI session.

### New dispatcher signature

`diagram::render_mermaid` changes return type:

```rust
pub enum DiagramError {
    ParseFailed { reason: String },
    RenderFailed { message: String },
}

pub fn render_mermaid(
    code: &str,
    theme: &Theme,
) -> Result<(Vec<Vec<StyledSpan>>, usize), DiagramError>;
```

The dispatcher wraps each renderer call in `catch_unwind`. A panic is
converted to `DiagramError::RenderFailed { message: panic_payload }`.

Individual renderers (`graph::state::render`, `graph::class::render`,
`graph::er::render`, `graph::mindmap::render`, and the existing
flowchart and sequence renderers) continue to return
`Option<(Vec<Vec<StyledSpan>>, usize)>` — `None` means "could not parse
or lay out this input". The dispatcher turns `None` into
`DiagramError::ParseFailed { reason }` where `reason` is derived from
the keyword being dispatched (e.g. `"could not parse classDiagram"`).
This keeps per-renderer code simple while still giving the user a
coarse, type-specific diagnostic. If a parser can cheaply detect a more
specific cause (e.g. `"empty diagram body"`, `"unterminated class body"`)
it may return that instead via an `Option<String>` reason side-channel,
but rich line-numbered diagnostics are explicitly out of scope for
Batch G — the source block beneath the banner is the diagnostic.

Existing internal tests use `.expect("…")` on the success path; that
works unchanged on `Result`. Tests asserting failure switch from
`assert!(render_mermaid(...).is_none())` to
`assert!(matches!(render_mermaid(...), Err(_)))`.

### Native-first dispatch

Today `src/markdown.rs:274-303` checks `MermaidMode` *first*. In
`MermaidMode::Image` it calls `mermaid_image_url` unconditionally —
without ever consulting `render_mermaid`. That means a native renderer
added in this batch would be silently bypassed for every
Kitty/iTerm2/Sixel user (the population most affected by the broken
mermaid.ink path). Batch G must change this.

New dispatch in `src/markdown.rs` for `lang == "mermaid"`:

1. Call `diagram::render_mermaid(&code, self.theme)` first.
2. On `Ok(rows, width)`, emit the native ASCII diagram block via
   `emit_diagram_block` regardless of `mermaid_mode`. Native wins.
3. On `Err(DiagramError)`, call a new `emit_diagram_error_block(reason)`
   that renders a labeled banner describing the failure, then fall
   through to the normal code-block rendering path so the original
   mermaid source is displayed underneath. `MermaidMode` is ignored in
   this branch — the source block is the user-visible fallback on every
   terminal protocol.
4. Only when `render_mermaid` is not invoked at all (because the type is
   still in `is_unsupported_diagram`) does the existing `MermaidMode`
   match decide between `emit_image_block(mermaid_image_url(...))`
   (broken until Batch R) and the source block.

After this change, every type with a native renderer (flowchart,
sequence, and the four new ones) is rendered natively on every terminal
protocol. The `mermaid.ink` image path becomes a fallback only for the
~20 unported types, and disappears entirely in Batch R.

### Error banner presentation

The error banner reuses the existing `emit_diagram_block` border style
(`src/markdown.rs:474`) with a distinct label so users can tell at a
glance what happened:

```
  ╭─ mermaid (render error: unsupported syntax in classDiagram) ───────╮
  │ <no diagram content — see source below>                            │
  ╰────────────────────────────────────────────────────────────────────╯
```

The label uses a new theme color `diagram_error_fg` (red-tinted in dark
theme, dark-red in light theme) so it stands out from the neutral
`code_label` of a successful render. The banner is one row tall (top
border + bottom border, no content rows) to minimize vertical noise;
the actual diagnostic information lives in the label and the source
block beneath.

The source block follows immediately, rendered through the normal
syntect path with the existing ` mermaid (diagram) ` label — exactly
what users see today for unported types.

### Pipable output

The same fallback applies to piped/non-TTY output. A parse failure
during `mdterm README.md | less -R` produces the error banner followed
by the source code block, so pipes never silently swallow diagrams.

### Testing for failures

- **Parse failure path:** a deliberately malformed classDiagram
  (unterminated `class Foo {`) renders to an error banner whose label
  contains `"render error"` plus the source block containing `class
  Foo`.
- **Render failure path:** inject a renderer that always panics (via a
  `#[cfg(test)]` hook), assert the dispatcher returns
  `Err(RenderFailed { .. })` and that `markdown.rs` emits the error
  banner + source rather than crashing.
- **Native-first precedence:** `erDiagram` after Batch G renders via the
  native ASCII path even in `MermaidMode::Image`, while an unported
  type (e.g. `pie`) still emits an image block.

## Testing

The pure code-move phase must keep every existing test in `src/diagram.rs`
green without modification. Existing tests are split between
`graph/flowchart.rs` (flowchart/parser/LR routing tests) and `sequence.rs`
(sequence tests); the dispatch and `mermaid_image_url` tests stay in
`mod.rs`.

One existing test must change as part of Batch G, not the code-move phase:
`unsupported_diagram_falls_back_to_source` in `diagram.rs:2413` currently
asserts `classDiagram`, `erDiagram`, and `stateDiagram-v2` all return
`None`. After Batch G they return `Ok(...)` — the test moves to
`graph/{state,class,er}/rs` as positive smoke tests, and the dispatch
test in `mod.rs` is reduced to a single unported type (e.g. `pie`) to
keep proving the unsupported-type fallback path.

Per new renderer:

- **Parser unit tests:** tricky syntax — composite state bodies, generic
  types, crow's-foot tokens, mindmap indentation levels, cardinality
  quotes — parses to the expected structures.
- **Renderer smoke tests** in the existing style (`render_mermaid(input,
  &theme).expect(...)` then assert key strings/shapes appear): class name
  and member rows, entity fields and PK/FK badges, state initial/final
  glyphs, root + level-1 labels.
- **Layout regression tests:** at least one test per type asserts canvas
  width and height are positive and a known label appears at an expected
  position.
- **Edge-style backward compatibility:** existing flowchart/sequence tests
  must pass unchanged after the `draw_edge_*` signature change, proving
  the byte-identical-rendering claim.

Failure-path tests are covered above in *Error handling and dispatch
precedence → Testing for failures*.

## Risks

- **`draw_edge_*` signature change.** Switching from `Option<&str>` to
  `EdgeStyle` touches flowchart and sequence call sites. Mitigation: the
  migration is mechanical; existing tests are the regression check.
- **Composite state rendering.** Rendering an inner graph to a sub-canvas
  and embedding as a tall outer node is new; the sub-canvas may need a
  background tint (`composite_state_bg`). The fallback is layered: (1)
  attempt composite rendering with a sub-canvas; (2) if the result is
  clipped or overlapping, degrade to two-level flattening (no nested
  `state { }` blocks — children inlined into the parent layout); (3) if
  even flattening fails, return `None` and the dispatcher shows the
  error banner + source block. Layer 2 may ship before layer 1 if
  compositing proves finicky; the user always lands on a usable view.
- **Mindmap indent measurement.** Measuring indent from the first
  non-root line is heuristic. If real-world mindmap files mix tabs and
  spaces, the parser may misjudge depth. Mitigation: treat each leading
  whitespace run as one level, regardless of width; document this.
- **Crow's-foot glyph width.** The `⟨` / `⟩` glyphs may not render in
  every font. Mitigation: provide a fallback to `<` / `>` if glyph width
  detection in the viewer reports an issue, and call out the recommended
  font set in the README.

## Open questions

None blocking. The four composite-state risk and the mindmap indent
heuristic are flagged above with fallbacks.
