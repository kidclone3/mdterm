# State-Diagram Render Fixes Design

## Goal

Fix the three rendering bugs that `demo/state-diagram.md` surfaced when its
`stateDiagram-v2` block is rendered by mdterm's native ASCII renderer, so the
demo is screenshot-worthy and the inline renderer tests cover the regressions.

## Background

mdterm ships a native ASCII renderer for `stateDiagram` / `stateDiagram-v2`
(`src/diagram/graph/state.rs`, introduced in Batch G). A smoke test against
`demo/state-diagram.md` (a state machine with an initial pseudo-state, seven
plain states, one composite state, a multi-line note, and a final pseudo-state)
surfaced three bugs:

1. **Multi-line notes are not parsed.** The note block
   `note right of Paid … end note` is rendered as three stray boxes labelled
   `Funds`, `by`, `end` next to the initial pseudo-state, because the parser
   only knows the colon form (`note right of X : text`) and falls through,
   treating the note body and the literal `end note` as bare state
   declarations.
2. **Composite state layout overflows.** The `Cancelled` composite's reserved
   size doesn't match its stamped size; its inner content bleeds past the outer
   rectangle and external↔composite edges dive into the interior instead of
   landing on the border.
3. **Long down-edge labels overrun.** `label_printed` and `carrier_dropoff`
   are written rightward from `src_cx + 2` by `Canvas::draw_edge_td` with no
   width check and no horizontal reservation, so they collide with whatever
   sits in the next column (split as `labe│_printed`).

All three are localised to `src/diagram/graph/state.rs` and
`src/diagram/canvas.rs`. No other diagram type uses this code path.

## Scope

In scope:

- Teach `parse_state_diagram` the multi-line `note SIDE of TARGET … end note`
  block form, and render multi-line note text in a single box positioned beside
  the target state.
- Replace the flat `edge_gap` and `h_gap` constants in `render_state_canvas`
  with per-gap and per-layer values sized from the longest relevant edge label,
  so down-edge labels never reach a neighbouring box.
- Attach external↔composite edges to the composite's top or bottom border
  instead of its geometric centre; size composites to include incident edge
  labels; and clip inner stamps to the composite's interior rectangle.
- Add inline `#[cfg(test)]` regression tests for each fix.

Out of scope:

- Replacing the layer-based layout with a dagre-style engine (deferred; the
  targeted reservation above resolves the reported symptoms without it).
- Touching `flowchart`, `classDiagram`, `erDiagram`, `mindmap`, or
  `sequenceDiagram` renderers.
- Changing the public API `pub fn render_mermaid(code: &str, theme: &Theme)
  -> Option<(Vec<Vec<StyledSpan>>, usize)>`.
- Any new dependency.

## Architecture

### Bug 1 — Multi-line note parser and renderer

**Parser** (`state.rs`, `parse_state_diagram` around line 315). Today the
`note SIDE of TARGET` branch calls `parse_state_note` (line 237) which only
accepts the colon form. Restructure:

- On `note SIDE of TARGET` (any of `left/right/over`), peek the remainder:
  - If it contains `:` → existing behaviour: single-line note with text after
    the colon.
  - Else → block form: enter a collection loop, consuming lines until a line
    whose trimmed value equals `end note` (tolerate bare `end` for v1
    compatibility). Join the trimmed body lines with `\n` into
    `StateNote::text`. Register the target state and push the note.
- The block-form collector mirrors the existing `collect_block_body` helper
  used for composite `state Foo { … }` bodies (line 350), so the code style
  is consistent.

**Renderer** (`state.rs` lines 615–645, `canvas.rs`). The current note loop
calls `Canvas::draw_node` with a single-line label. Replace with:

- Split `note.text` on `\n`. Compute `note_w = max(line_chars) + 4` and
  `note_h = num_lines + 2`.
- Add a new `Canvas::draw_note_card(left_x, top_y, width, height, lines:
  &[&str], border_fg, text_fg)` next to `draw_node_with_height`
  (`canvas.rs:318`). It draws a rounded rectangle (reusing the same
  box-character set as `draw_node_with_height`'s `Rounded` arm) and centres
  each entry of `lines` on its own row, padded with one blank row at the top
  and bottom of the interior. No existing canvas primitive stacks arbitrary
  text lines today: `draw_node_with_height` is single-line, and `draw_card`
  (`canvas.rs:473`) is JSON-view-specific (title + `CardDrawRow` key/value
  pairs), so a dedicated primitive is warranted.
- Replace the `has_notes` flat padding reserve (`state.rs:514–517`) with a
  measured value: scan notes for the maximum box width on each side and
  reserve `max_left_note_w + 2` and `max_right_note_w + 2` independently, plus
  `max(note_h) + 1` top padding when any `NoteSide::Over` note exists. Drop
  the flat `18`/`3` constants.

### Bugs 2 & 3 — Targeted layout reservation

Three pieces, all inside `render_state_canvas` plus one new `Canvas` method.
No change to `graph/mod.rs` layer assignment or ordering.

**Piece A — per-inter-layer vertical gap.** Replace `edge_gap = 4`
(`state.rs:487`, `:512`, `:591`) with `layer_gaps: Vec<usize>` of length
`layers.len() - 1`. For each gap `i`, scan the edges whose `from` node is in
`layers[i]` and whose `to` node is in `layers[i+1]`; set
`layer_gaps[i] = max(4, longest_label_chars + 2)`. Use it in
`total_height = sum(layer_heights) + sum(layer_gaps)` and in the per-layer
`y += layer_height + layer_gaps[i]` advancement. Result: every down-edge
label has its own row inside the span, so `draw_edge_td`'s rightward write at
`canvas.rs:656` cannot reach the destination box.

**Piece B — per-layer horizontal gap.** Replace `h_gap = 4` (`state.rs:486`,
`:491`, `:541`) with `h_gap_for(layer)`. For each layer, take the longest
label among edges whose `from` node sits in that layer; set
`h_gap = max(4, longest_label_chars + 2)`. Use the per-layer value in the
`layer_width` sum (line 491–498) and in the `centers` computation (line
541–550). Same-layer neighbours (e.g. `Paid` and the `Cancelled` composite)
are pushed apart far enough that a down-edge label drawn beside one node's
vertical arrow cannot reach the neighbour's box.

**Piece C — composite border attach + inner clip.**

1. **Border attachment.** In the edge loop (`state.rs:595–613`), when `from`
   or `to` is a key in `diagram.composites`, substitute the border y for
   that endpoint (`top_y` for incoming edges, `top_y + height` for outgoing)
   instead of `center_y`. Arrowheads land on the rectangle edge.
2. **Composite size includes incident edge labels.** When sizing composites
   (`state.rs:475`), compute
   `composite_w = max(inner_w + 4, longest_incident_edge_label + 4)` where
   the max is over edges whose `from` or `to` equals the composite id.
   Prevents labels like `user_cancel` being squeezed against the border.
3. **Inner stamp clip.** Add `Canvas::stamp_canvas_clipped(other, dx, dy,
   max_w, max_h)` next to `stamp_canvas` (`canvas.rs:438`); it copies cells
   from `other` into `self` only where `dx <= x < dx + max_w` and
   `dy <= y < dy + max_h`. Use it at `state.rs:572` with
   `max_w = w.saturating_sub(4)` and `max_h = h.saturating_sub(3)`. Any
   inner content that would overflow the reserved interior is clipped rather
   than corrupting neighbours — bounds the failure mode to "composite looks
   cropped", never "next layer is clobbered".

### Verification

Build bar (run after each piece):

- `cargo fmt`
- `cargo clippy -- -D warnings`
- `cargo test`
- `cargo build --release`

Manual smoke (reproduces the comparison from the bug report):

- `COLUMNS=120 ./target/release/mdterm demo/state-diagram.md >
  /tmp/opencode/mdterm_render.txt`
- Re-render to PNG and compare against the mermaid.ink PNG. Acceptance bar:
  every transition label renders as one contiguous substring; the note reads
  `Funds captured` / `by the gateway` beside `Paid`; the `Cancelled`
  composite's outer rectangle fully contains `[*]`, `Refunded`, and the inner
  arrows; nothing bleeds into the `Packed`/`Shipped`/`Delivered` column.

## Tests

Inline `#[cfg(test)]` tests in `state.rs`, next to the existing
`parses_note_placements` (line 738) and `renders_note_text` (line 800).

Bug 1:

- `parses_multiline_note_block` — input is the demo's `note right of Paid`
  block; asserts `notes[0].text == "Funds captured\nby the gateway"` and that
  no nodes named `Funds`, `by`, or `end` exist.
- `renders_multiline_note_beside_target` — full render of the note snippet;
  asserts both `Funds` and `gateway` appear and `end` does not.
- `parses_note_block_then_transition` — note block followed by a real
  transition; asserts the parser resumes cleanly.

Bugs 2 & 3:

- `renders_long_edge_label_unclipped` — minimal
  `stateDiagram-v2\n[*] --> A\nA --> B : a_very_long_event\nB --> [*]`;
  asserts the substring `a_very_long_event` is contiguous in the output.
- `composite_external_edge_attaches_to_border` — arrow into a composite ends
  on the composite's top-border row, not inside.
- `composite_inner_clipped_to_bounds` — render the demo's `Cancelled`
  composite alone; assert no non-space glyph appears outside the outer
  rectangle's column range for any row inside its row range.
- `demo_renders_cleanly` — render the actual `demo/state-diagram.md` mermaid
  block; asserts (a) all eight state names appear, (b) all seven transition
  labels appear as contiguous substrings (`label_printed`, `carrier_dropoff`,
  `warehouse_pick`, etc.), (c) `end` does not appear as a standalone token,
  (d) every emitted row's length matches the reported canvas width (no
  out-of-bounds writes).

## Risk & Rollout

- All changes are inside `src/diagram/` — no public API change, no dependency
  change, no config-file impact.
- `render_mermaid()` already runs each renderer under `catch_unwind`
  (`diagram/mod.rs`), so any residual regression degrades to the inline
  `mermaid (render error: …)` banner rather than crashing the TUI. Blast
  radius is bounded to "this diagram looks bad", not "mdterm crashes".
- No other diagram type uses the state-diagram code path; `classDiagram`,
  `erDiagram`, `flowchart`, `mindmap`, `sequenceDiagram` are unaffected.
- Single commit — `fix(mermaid): state-diagram notes, composite layout,
  edge-label reservation` — because the three fixes share test fixtures (the
  demo) and the layout-reservation pieces (A/B/C) aren't independently
  meaningful. New inline tests ship in the same commit.
