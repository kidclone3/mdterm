# State-Diagram Render Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the three state-diagram rendering bugs in `demo/state-diagram.md` (multi-line notes, composite overflow, long-label overflow) with targeted layout reservation, plus regression tests.

**Architecture:** All changes inside `src/diagram/`. Parser gains multi-line `note … end note` block support. A new `Canvas::draw_note_card` primitive renders multi-line note text. `render_state_canvas` replaces flat `h_gap`/`edge_gap` constants with per-layer/per-gap values sized from edge labels. Composite external↔inner edges attach to the composite border; composites are sized to fit incident edge labels; a new `Canvas::stamp_canvas_clipped` bounds inner stamps to the composite interior.

**Tech Stack:** Rust edition 2024, existing `pulldown-cmark` / `crossterm` / `syntect` stack. No new dependencies.

## Global Constraints

- Rust edition 2024 (rustc 1.85+).
- No new dependencies; `Cargo.toml` unchanged.
- Public API `pub fn render_mermaid(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)>` unchanged.
- Inline `#[cfg(test)] mod tests` is the test harness — there is no `tests/` directory for these modules. Run with `cargo test`.
- Style bar: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release` all clean.
- `render_mermaid()` runs renderers under `catch_unwind`, so a panic degrades to an inline banner — never crashes the TUI.
- Commit per task during execution; the per-task commits may be squashed on merge if desired.

## File Structure

- `src/diagram/graph/state.rs` (850 lines) — parser + renderer for `stateDiagram`/`stateDiagram-v2`. Modified by Tasks 1, 3, 4, 5, 6, 7.
- `src/diagram/canvas.rs` (1175 lines) — `Canvas` grid + drawing primitives. Modified by Tasks 2 (new `draw_note_card`), 6 (new `stamp_canvas_clipped`).
- `demo/state-diagram.md` — unchanged; used as the integration-test fixture in Task 7.

Each task adds inline `#[test]` functions to the existing `mod tests` at the bottom of `state.rs` (or `canvas.rs` for canvas-primitive tests).

---

### Task 1: Multi-line note parser

**Files:**
- Modify: `src/diagram/graph/state.rs:237-260` (`parse_state_note`), `:314-327` (note dispatch in `parse_state_diagram`)

**Interfaces:**
- Consumes: existing `StateNote { target, side, text }` (state.rs:81), `parse_state_note` single-line helper
- Produces: `parse_state_note` accepts the block form by collecting body lines into `text` joined with `\n`. `StateNote.text` may now contain newlines.

- [ ] **Step 1: Write the failing test**

Add to `src/diagram/graph/state.rs` `mod tests` (after `parses_note_placements` at line 747):

```rust
#[test]
fn parses_multiline_note_block() {
    let src = "stateDiagram-v2\n[*] --> Paid\nnote right of Paid\n    Funds captured\n    by the gateway\nend note\nPaid --> [*]";
    let d = parse_state_diagram(src).unwrap();
    assert_eq!(d.notes.len(), 1, "exactly one note, got: {:?}", d.notes);
    assert_eq!(d.notes[0].target, "Paid");
    assert_eq!(d.notes[0].side, NoteSide::Right);
    assert_eq!(
        d.notes[0].text,
        "Funds captured\nby the gateway",
        "block body should join trimmed lines with \\n"
    );
    // The body lines and the literal `end note` must NOT become states.
    for forbidden in ["Funds", "captured", "gateway", "end", "note"] {
        assert!(
            !d.nodes.contains_key(forbidden),
            "lexeme `{forbidden}` leaked into nodes: {:?}",
            d.nodes.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn parses_note_block_then_transition() {
    let src = "stateDiagram-v2\n[*] --> A\nnote right of A\n  body line\nend note\nA --> B";
    let d = parse_state_diagram(src).unwrap();
    assert_eq!(d.notes.len(), 1);
    assert_eq!(d.notes[0].text, "body line");
    assert_eq!(d.edges.len(), 2, "parser should resume after the note block");
    assert!(d.nodes.contains_key("B"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state::tests::parses_multiline_note_block state::tests::parses_note_block_then_transition`
Expected: FAIL — the parser falls through, `notes` is empty (or has empty text), and `Funds`/`end` appear as nodes.

- [ ] **Step 3: Write minimal implementation**

Replace the note-dispatch branch in `parse_state_diagram` (state.rs:315-327) with block-form handling. Replace:

```rust
        // Notes.
        if trimmed.starts_with("note ") {
            if let Some(note) = parse_state_note(trimmed) {
                register_state(
                    &mut nodes,
                    &mut node_order,
                    &note.target,
                    &note.target,
                    StateKind::Normal,
                );
                notes.push(note);
            }
            continue;
        }
```

with:

```rust
        // Notes.
        if trimmed.starts_with("note ") {
            // Two forms: single-line `note SIDE of TARGET : text` and block
            //   note SIDE of TARGET
            //     body line
            //     ...
            //   end note
            if let Some(note) = parse_state_note(trimmed) {
                let mut note = note;
                if note.text.is_empty() && i < lines.len() {
                    // Block form: collect body until `end note` (or bare `end`).
                    let mut body: Vec<String> = Vec::new();
                    while i < lines.len() {
                        let blk = lines[i].trim();
                        i += 1;
                        if blk == "end note" || blk == "end" {
                            break;
                        }
                        if blk.is_empty() {
                            continue;
                        }
                        body.push(blk.to_string());
                    }
                    note.text = body.join("\n");
                }
                register_state(
                    &mut nodes,
                    &mut node_order,
                    &note.target,
                    &note.target,
                    StateKind::Normal,
                );
                notes.push(note);
            }
            continue;
        }
```

`parse_state_note` (state.rs:237-260) already returns `text: String::new()` for the no-colon case (line 250), so no change to that function is needed — the dispatcher now treats empty text as the block-form signal.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state::tests::parses_multiline_note_block state::tests::parses_note_block_then_transition state::tests::parses_note_placements`
Expected: PASS — all three note tests green (the existing single-line test must still pass).

- [ ] **Step 5: Commit**

```bash
git add src/diagram/graph/state.rs
git commit -m "fix(mermaid/state): parse multi-line note blocks

Teach parse_state_diagram the block form
  note SIDE of TARGET
    body
  end note
The single-line parse_state_note helper already returned empty text for
the no-colon case; the dispatcher now treats that as the block-form
signal and collects body lines until `end note`."
```

---

### Task 2: Multi-line note renderer (`Canvas::draw_note_card`)

**Files:**
- Modify: `src/diagram/canvas.rs` (add `draw_note_card` after `draw_node_with_height` at line 382)
- Modify: `src/diagram/graph/state.rs:514-517` (note padding reserve), `:615-645` (note render loop)

**Interfaces:**
- Consumes: `Canvas::set_node` (canvas.rs:217), `Theme::code_border`/`Theme::fg` (theme.rs:28, :7)
- Produces: `Canvas::draw_note_card(left_x, top_y, width, height, lines: &[&str], border_fg, text_fg)` — draws a rounded rectangle with each entry of `lines` centred on its own row.

- [ ] **Step 1: Write the failing test**

Add to `src/diagram/graph/state.rs` `mod tests` (after `renders_note_text` at line 812):

```rust
#[test]
fn renders_multiline_note_beside_target() {
    let rows = render_to_text(
        "stateDiagram-v2\n[*] --> Paid\nnote right of Paid\n    Funds captured\n    by the gateway\nend note\nPaid --> [*]",
    );
    let all: String = rows.join("\n");
    assert!(all.contains("Funds"), "first note line should appear, got:\n{all}");
    assert!(all.contains("gateway"), "second note line should appear, got:\n{all}");
    assert!(
        !all.split_whitespace().any(|t| t == "end"),
        "`end` keyword must not leak into render, got:\n{all}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state::tests::renders_multiline_note_beside_target`
Expected: FAIL — the old single-line note renderer is still in place; multi-line text won't render correctly (either shows nothing meaningful or splits into single-char rows).

- [ ] **Step 3: Write minimal implementation**

Add the new primitive to `src/diagram/canvas.rs` immediately after `draw_node_with_height` (after line 382):

```rust
    /// Draw a rounded rectangle note card with one or more centred text lines.
    /// Each entry of `lines` is placed on its own row inside the interior,
    /// padded by one blank row at the top and bottom. Used by stateDiagram
    /// multi-line notes. Bounds-checked: writes outside the canvas are
    /// silently dropped.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_note_card(
        &mut self,
        left_x: usize,
        top_y: usize,
        width: usize,
        height: usize,
        lines: &[&str],
        border_fg: Option<Color>,
        text_fg: Option<Color>,
    ) {
        if width < 4 || height < 3 || lines.is_empty() {
            return;
        }
        // Top border: ╭───╮
        self.set_node(left_x, top_y, '\u{256d}', border_fg);
        for x in (left_x + 1)..(left_x + width - 1) {
            self.set_node(x, top_y, '\u{2500}', border_fg);
        }
        self.set_node(left_x + width - 1, top_y, '\u{256e}', border_fg);

        // Interior rows: side borders + centred text lines.
        let interior_top = top_y + 1;
        let interior_bot = top_y + height - 1;
        // Reserve one blank row of padding at top and bottom; stack lines in between.
        let first_line_y = interior_top + 1;
        for row_idx in 0..(interior_bot - interior_top) {
            let row_y = interior_top + row_idx;
            self.set_node(left_x, row_y, '\u{2502}', border_fg);
            for x in (left_x + 1)..(left_x + width - 1) {
                self.set_node(x, row_y, ' ', text_fg);
            }
            let line_idx = row_idx as isize - 1; // -1 because of top padding row
            if line_idx >= 0 && (line_idx as usize) < lines.len() {
                let chars: Vec<char> = lines[line_idx as usize].chars().collect();
                let pad = (width - 2).saturating_sub(chars.len());
                let start = left_x + 1 + pad / 2;
                for (i, &ch) in chars.iter().enumerate() {
                    if start + i < left_x + width - 1 {
                        self.set_node(start + i, row_y, ch, text_fg);
                    }
                }
            }
            self.set_node(left_x + width - 1, row_y, '\u{2502}', border_fg);
        }

        // Bottom border: ╰───╯
        let bot_y = top_y + height - 1;
        self.set_node(left_x, bot_y, '\u{2570}', border_fg);
        for x in (left_x + 1)..(left_x + width - 1) {
            self.set_node(x, bot_y, '\u{2500}', border_fg);
        }
        self.set_node(left_x + width - 1, bot_y, '\u{256f}', border_fg);
    }
```

Then rewrite the note render loop in `src/diagram/graph/state.rs:615-645`. Replace:

```rust
    // Notes (simple side/over boxes positioned next to their target).
    for note in &diagram.notes {
        if let Some(target) = positions.get(&note.target) {
            let text_chars: Vec<char> = note.text.chars().collect();
            let note_w = text_chars.len().max(3) + 4;
            let label = if note.text.is_empty() { " " } else { &note.text };

            let (note_cx, note_y) = match note.side {
                NoteSide::Left => {
                    let left_x =
                        target.center_x.saturating_sub(target.width / 2 + note_w / 2 + 2);
                    (left_x + note_w / 2, target.top_y)
                }
                NoteSide::Right => {
                    let right_x = target.center_x + target.width / 2 + note_w / 2 + 2;
                    (right_x, target.top_y)
                }
                NoteSide::Over => (target.center_x, target.top_y.saturating_sub(3)),
            };

            canvas.draw_node(
                note_cx,
                note_y,
                note_w,
                label,
                NodeShape::Rectangle,
                border_fg,
                text_fg,
            );
        }
    }
```

with:

```rust
    // Notes (single- or multi-line boxes positioned beside their target).
    for note in &diagram.notes {
        if let Some(target) = positions.get(&note.target) {
            let lines: Vec<&str> = if note.text.is_empty() {
                vec![" "]
            } else {
                note.text.split('\n').collect()
            };
            let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let note_w = longest.max(3) + 4;
            let note_h = lines.len() + 2;

            let (note_left_x, note_top_y) = match note.side {
                NoteSide::Left => {
                    let left_x =
                        target.center_x.saturating_sub(target.width / 2 + note_w + 2);
                    (left_x, target.top_y)
                }
                NoteSide::Right => {
                    let right_x = target.center_x + target.width / 2 + 2;
                    (right_x, target.top_y)
                }
                NoteSide::Over => (
                    target.center_x.saturating_sub(note_w / 2),
                    target.top_y.saturating_sub(note_h + 1),
                ),
            };

            canvas.draw_note_card(
                note_left_x,
                note_top_y,
                note_w,
                note_h,
                &lines,
                border_fg,
                text_fg,
            );
        }
    }
```

And update the note padding reserve in `src/diagram/graph/state.rs:514-517`. Replace:

```rust
    // Reserve side + top padding so notes have somewhere to live.
    let has_notes = !diagram.notes.is_empty();
    let side_padding = if has_notes { 18 } else { 2 };
    let top_padding = if has_notes { 3 } else { 0 };
```

with:

```rust
    // Reserve side + top padding so notes have somewhere to live. Size from
    // the actual note boxes (longest line + 4 wide, n_lines + 2 tall) rather
    // than a flat guess.
    let (left_pad, right_pad, over_pad) = note_padding(&diagram.notes);
    let side_padding = left_pad.max(right_pad).max(2);
    let top_padding = over_pad;
```

Add a small helper near the top of the renderer section (after `to_graph`, around state.rs:453):

```rust
/// Compute left/right/over canvas padding required by the diagram's notes.
/// Returns (left_pad, right_pad, over_top_pad). Each side pad is the max
/// note width on that side plus a 2-column gap; over_top_pad is the max
/// over-note height plus one row when any over-note exists, else 0.
fn note_padding(notes: &[StateNote]) -> (usize, usize, usize) {
    let mut left = 2usize;
    let mut right = 2usize;
    let mut over = 0usize;
    let mut has_over = false;
    for n in notes {
        let lines: Vec<&str> = if n.text.is_empty() {
            vec![" "]
        } else {
            n.text.split('\n').collect()
        };
        let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let w = longest.max(3) + 4 + 2; // box + gap
        let h = lines.len() + 2 + 1;    // box + gap row
        match n.side {
            NoteSide::Left => left = left.max(w),
            NoteSide::Right => right = right.max(w),
            NoteSide::Over => {
                has_over = true;
                over = over.max(h);
            }
        }
    }
    let over_final = if has_over { over } else { 0 };
    (left, right, over_final)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state::tests::renders_multiline_note_beside_target state::tests::renders_note_text state::tests::parses_note_placements`
Expected: PASS — new multi-line test passes; existing single-line note test still passes.

- [ ] **Step 5: Commit**

```bash
git add src/diagram/canvas.rs src/diagram/graph/state.rs
git commit -m "fix(mermaid/state): render multi-line note blocks

Add Canvas::draw_note_card primitive (rounded box with one centred
text line per interior row) and rewrite the state-diagram note loop
to use it. Note padding reserve is now sized from actual note boxes
instead of a flat 18/3 guess."
```

---

### Task 3: Per-layer horizontal gap from down-edge labels (Piece B)

**Files:**
- Modify: `src/diagram/graph/state.rs:486` (h_gap constant), `:490-498` (layer width sum), `:534-592` (per-layer placement)

**Interfaces:**
- Consumes: `diagram.edges`, `layers`, per-node `widths`
- Produces: a per-layer `h_gap` value used both in `max_layer_width` and in the placement loop.

- [ ] **Step 1: Write the failing test**

Add to `src/diagram/graph/state.rs` `mod tests`:

```rust
#[test]
fn renders_long_edge_label_unclipped() {
    // Two same-layer sources (A, X) whose down-edges carry long labels.
    // A --> B : a_very_long_event must render as a contiguous substring;
    // X must be pushed right enough that the label doesn't bisect X's box.
    let rows = render_to_text(
        "stateDiagram-v2\n[*] --> A\n[*] --> X\nA --> B : a_very_long_event\nX --> Y\nB --> [*]\nY --> [*]",
    );
    let all: String = rows.join("\n");
    assert!(
        all.contains("a_very_long_event"),
        "long edge label must render unsplit, got:\n{all}"
    );
    // Sanity: both target states appear.
    assert!(all.contains('B'), "target state B missing, got:\n{all}");
    assert!(all.contains('X'), "sibling state X missing, got:\n{all}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state::tests::renders_long_edge_label_unclipped`
Expected: FAIL — current flat `h_gap = 4` leaves no room for the 18-char label beside A's down-arrow; `a_very_long_event` is split by X's box border.

- [ ] **Step 3: Write minimal implementation**

Add a helper (near `note_padding` from Task 2):

```rust
/// Longest chars() count among edge labels whose `from` node sits in `layer`
/// and whose `to` node sits in the next layer (`layers[layer_idx + 1]`).
/// Used to reserve horizontal room so down-edge labels drawn rightward from
/// the arrow never reach a same-layer neighbour's box.
fn layer_down_edge_label_max(
    diagram: &StateDiagram,
    layers: &[Vec<String>],
    layer_idx: usize,
) -> usize {
    let next = match layers.get(layer_idx + 1) {
        Some(n) => n,
        None => return 0,
    };
    let mut m = 0usize;
    for id in &layers[layer_idx] {
        for e in &diagram.edges {
            if &e.from == id
                && next.iter().any(|n| n == &e.to)
                && let Some(lbl) = &e.label
            {
                m = m.max(lbl.chars().count());
            }
        }
    }
    m
}
```

Replace the `h_gap` constant and its uses in `render_state_canvas`. At state.rs:486 remove `let h_gap = 4;`. Update the `max_layer_width` loop (state.rs:490-498) — replace:

```rust
    let mut max_layer_width = 0;
    for layer in &layers {
        let w: usize = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .sum::<usize>()
            + layer.len().saturating_sub(1) * h_gap;
        max_layer_width = max_layer_width.max(w);
    }
```

with:

```rust
    let layer_h_gaps: Vec<usize> = (0..layers.len())
        .map(|i| {
            let lbl = layer_down_edge_label_max(diagram, &layers, i);
            lbl.saturating_add(2).max(4)
        })
        .collect();
    let mut max_layer_width = 0;
    for (idx, layer) in layers.iter().enumerate() {
        let h_gap = layer_h_gaps[idx];
        let w: usize = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .sum::<usize>()
            + layer.len().saturating_sub(1) * h_gap;
        max_layer_width = max_layer_width.max(w);
    }
```

Update the placement loop (state.rs:534-592). Inside `for (layer_idx, layer) in layers.iter().enumerate()`, replace:

```rust
        let node_widths: Vec<usize> = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .collect();
        let layer_width: usize = node_widths.iter().sum::<usize>()
            + layer.len().saturating_sub(1) * h_gap;
```

with:

```rust
        let h_gap = layer_h_gaps[layer_idx];
        let node_widths: Vec<usize> = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .collect();
        let layer_width: usize = node_widths.iter().sum::<usize>()
            + layer.len().saturating_sub(1) * h_gap;
```

(The `centers` accumulation at state.rs:547-550 uses `h_gap` too — it now resolves to the local `h_gap` shadow.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state::tests`
Expected: PASS — the new test plus all existing state-diagram tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/diagram/graph/state.rs
git commit -m "fix(mermaid/state): reserve per-layer h_gap from down-edge labels

Replace the flat h_gap=4 with a per-layer value sized from the longest
label among edges crossing into the next layer. Same-layer neighbours
are pushed apart far enough that a label drawn rightward from a
vertical arrow can't reach a neighbour's box border."
```

---

### Task 4: Per-inter-layer vertical gap from crossing labels (Piece A)

**Files:**
- Modify: `src/diagram/graph/state.rs:487` (edge_gap constant), `:500-512` (total_height), `:591` (y advancement)

**Interfaces:**
- Consumes: `diagram.edges`, `layers`
- Produces: a `layer_gaps: Vec<usize>` parallel to the layers, summed into `total_height` and applied per-layer.

- [ ] **Step 1: Write the failing test**

Add to `src/diagram/graph/state.rs` `mod tests`:

```rust
#[test]
fn layer_gap_grows_with_edge_label_length() {
    // A long-label edge between layers 1 and 2 should produce a gap >= 7
    // (label_len + 2 = 8, more than the floor of 4). We assert it indirectly
    // by counting blank rows between the source and destination boxes.
    let rows = render_to_text(
        "stateDiagram-v2\n[*] --> A\nA --> B : quite_a_long_event\nB --> [*]",
    );
    let all: String = rows.join("\n");
    // Source label appears, dest label appears, and crucially the label
    // itself fits beside the arrow without overwriting B's top border.
    assert!(all.contains("quite_a_long_event"));
    assert!(all.contains('B'));
    // B's rounded top-border corner must appear AFTER the label row, not on it.
    let label_row = rows.iter().position(|r| r.contains("quite_a_long_event")).unwrap();
    let b_top_row = rows
        .iter()
        .position(|r| r.contains('\u{256d}') && r.contains('B'))
        .or_else(|| rows.iter().position(|r| r.contains('B')));
    assert!(
        b_top_row.map(|r| r > label_row).unwrap_or(true),
        "B's box must start below the label row, got rows {} vs {}",
        label_row,
        b_top_row.unwrap_or(0)
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state::tests::layer_gap_grows_with_edge_label_length`
Expected: may PASS already (the flat 4-row gap happens to place B below the label row in this minimal case) or FAIL (depending on whether the label is wide enough to wrap into the next row). If it passes, weaken the assertion to require the gap to be at least `max(4, label_len + 2)` by counting blank rows. If the test already passes, keep it as a regression guard and proceed — the implementation is still warranted for the symmetric invariant.

- [ ] **Step 3: Write minimal implementation**

Add a helper (next to `layer_down_edge_label_max`):

```rust
/// Same as layer_down_edge_label_max but returns the value to use as the
/// inter-layer vertical gap: `max(4, longest_label + 2)`. Per-gap rather
/// than global so unrelated layers stay compact.
fn layer_gap_size(
    diagram: &StateDiagram,
    layers: &[Vec<String>],
    layer_idx: usize,
) -> usize {
    layer_down_edge_label_max(diagram, layers, layer_idx)
        .saturating_add(2)
        .max(4)
}
```

In `render_state_canvas`, remove `let edge_gap = 4;` at state.rs:487. Compute `layer_gaps` alongside `layer_h_gaps`:

```rust
    let layer_gaps: Vec<usize> = (0..layers.len().saturating_sub(1))
        .map(|i| layer_gap_size(diagram, &layers, i))
        .collect();
```

Replace `total_height` (state.rs:511-512):

```rust
    let total_height: usize =
        layer_heights.iter().sum::<usize>() + layers.len().saturating_sub(1) * edge_gap;
```

with:

```rust
    let total_height: usize =
        layer_heights.iter().sum::<usize>() + layer_gaps.iter().sum::<usize>();
```

Replace the y-advancement at state.rs:591:

```rust
        y += layer_height + edge_gap;
```

with:

```rust
        let gap = if layer_idx + 1 < layer_gaps.len() {
            layer_gaps[layer_idx]
        } else {
            0
        };
        y += layer_height + gap;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state::tests`
Expected: PASS — all state-diagram tests green.

- [ ] **Step 5: Commit**

```bash
git add src/diagram/graph/state.rs
git commit -m "fix(mermaid/state): reserve per-inter-layer vertical gap from labels

Replace flat edge_gap=4 with per-gap values sized from the longest
label among edges crossing that gap. Symmetric invariant to the
horizontal h_gap reservation; future-proofs against multi-row label
wrapping and far_label placement on TD edges."
```

---

### Task 5: Composite width includes incident edge labels (Piece C 1+3)

**Files:**
- Modify: `src/diagram/graph/state.rs:470-484` (composite sizing block)

**Interfaces:**
- Consumes: `diagram.composites`, `diagram.edges`
- Produces: composite widths account for the longest incident edge label.

**Note on scope:** The original Piece C part 1 (border-attach) is a no-op
today because `NodeLayout.top_y` already points at the composite's border
row, so `draw_edge_td` already lands arrowheads on the border. That sub-change
is dropped from this task; only the size reservation (which does change
behaviour) is implemented. The `composite_external_edge_attaches_to_border`
test is kept as a regression guard in case border geometry shifts later.

- [ ] **Step 1: Write the failing test**

Add to `src/diagram/graph/state.rs` `mod tests`:

```rust
#[test]
fn composite_width_includes_incident_edge_label() {
    // `user_cancel` is a 12-char label on the Created -> Cancelled edge.
    // The composite's rendered width must accommodate it (no border clipping
    // of the label). We assert the label appears as a contiguous substring.
    let rows = render_to_text(
        "stateDiagram-v2\n[*] --> Created\nCreated --> Cancelled : user_cancel\nstate Cancelled {\n  [*] --> Refunded\n  Refunded --> [*]\n}\nCancelled --> [*]\nCreated --> [*]",
    );
    let all: String = rows.join("\n");
    assert!(
        all.contains("user_cancel"),
        "incident edge label must render unsplit beside the composite, got:\n{all}"
    );
}

#[test]
fn composite_external_edge_attaches_to_border() {
    // Regression guard: incoming arrowhead must land on the composite's
    // top-border row (not inside the interior). Today's geometry already
    // satisfies this because NodeLayout.top_y points at the border.
    let rows = render_to_text(
        "stateDiagram-v2\n[*] --> Outer\nstate Outer {\n  Inner1 --> Inner2\n}\nOuter --> [*]",
    );
    let all: String = rows.join("\n");
    assert!(all.contains("Outer"), "composite title should render");
    let top_row_idx = rows
        .iter()
        .position(|r| r.contains("Outer") && r.contains('\u{256d}'))
        .expect("composite top border row should exist");
    // The arrow into Outer (\u{25bc}) must appear on a row at or above the top border.
    let arrow_rows: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.contains('\u{25bc}'))
        .map(|(i, _)| i)
        .collect();
    assert!(
        arrow_rows.iter().any(|&r| r <= top_row_idx + 1),
        "incoming arrowhead must land on the composite top border (rows {:?}, top at {})",
        arrow_rows,
        top_row_idx
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state::tests::composite_width_includes_incident_edge_label state::tests::composite_external_edge_attaches_to_border`
Expected: `composite_width_includes_incident_edge_label` FAILs (composite too narrow, label split by border); `composite_external_edge_attaches_to_border` may already PASS (regression guard).

- [ ] **Step 3: Write minimal implementation**

Replace the composite-sizing block in `render_state_canvas` (state.rs:470-484). Replace:

```rust
    for (id, sn) in &diagram.nodes {
        if let Some(comp) = diagram.composites.get(id)
            && let Some(inner_diagram) = parse_state_diagram(&comp.source)
            && let Some((inner_canvas, inner_w)) = render_state_canvas(&inner_diagram, theme)
        {
            let w = inner_w + 4;
            let h = inner_canvas.height + 3;
            widths.insert(id.clone(), w);
            heights.insert(id.clone(), h);
            subcanvases.insert(id.clone(), inner_canvas);
            continue;
        }
        widths.insert(id.clone(), sn.box_width(&diagram.edges, id));
        heights.insert(id.clone(), 3);
    }
```

with:

```rust
    for (id, sn) in &diagram.nodes {
        if let Some(comp) = diagram.composites.get(id)
            && let Some(inner_diagram) = parse_state_diagram(&comp.source)
            && let Some((inner_canvas, inner_w)) = render_state_canvas(&inner_diagram, theme)
        {
            // Width must cover the inner canvas AND any incident edge label
            // (label sits beside the composite's exterior, so add label_len + 4).
            let incident_label = diagram
                .edges
                .iter()
                .filter(|e| &e.from == id || &e.to == id)
                .filter_map(|e| e.label.as_deref())
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0);
            let w = (inner_w + 4).max(incident_label + 4);
            let h = inner_canvas.height + 3;
            widths.insert(id.clone(), w);
            heights.insert(id.clone(), h);
            subcanvases.insert(id.clone(), inner_canvas);
            continue;
        }
        widths.insert(id.clone(), sn.box_width(&diagram.edges, id));
        heights.insert(id.clone(), 3);
    }
```

No edge-loop change in this task — the existing `dst.top_y` for composite destinations already correctly targets the border row.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state::tests`
Expected: PASS — both new tests and all existing tests green.

- [ ] **Step 5: Commit**

```bash
git add src/diagram/graph/state.rs
git commit -m "fix(mermaid/state): composite width includes incident edge labels

Composite nodes now size their width to fit the longest incident edge
label (plus padding) in addition to the inner canvas, so labels like
user_cancel no longer get clipped by the composite border."
```

---

### Task 6: `stamp_canvas_clipped` primitive (Piece C 2)

**Files:**
- Modify: `src/diagram/canvas.rs:438-461` (add `stamp_canvas_clipped` next to `stamp_canvas`)
- Modify: `src/diagram/graph/state.rs:572` (use the clipped variant)

**Interfaces:**
- Consumes: `stamp_canvas` semantics
- Produces: `Canvas::stamp_canvas_clipped(other, dx, dy, max_w, max_h)` — copies only cells inside `[dx, dx+max_w) × [dy, dy+max_h)`.

- [ ] **Step 1: Write the failing test**

Add to `src/diagram/graph/state.rs` `mod tests`:

```rust
#[test]
fn composite_inner_clipped_to_bounds() {
    // Force the inner diagram to be wider than the composite's interior
    // would naturally allow (long inner state name + nested edge label),
    // then assert nothing bleeds outside the composite's column range.
    let rows = render_to_text(
        "stateDiagram-v2\n[*] --> C\nstate C {\n  [*] --> InnerWithLongName\n  InnerWithLongName --> [*] : a_long_inner_event\n}\nC --> [*]",
    );
    // Find the composite's outer rectangle column range by locating the
    // top-border corners \u{256d} (top-left) and \u{256e} (top-right).
    let top_idx = rows
        .iter()
        .position(|r| r.contains('C') && r.contains('\u{256d}'))
        .expect("composite top border row");
    let top_row = &rows[top_idx];
    let tl = top_row.find('\u{256d}').expect("top-left corner");
    let tr = top_row.rfind('\u{256e}').expect("top-right corner");
    // For every row that lies inside the composite's vertical span, no
    // non-space glyph may appear outside [tl, tr].
    for (row_off, row) in rows.iter().enumerate().skip(top_idx) {
        // Stop at the bottom border \u{2570}.
        if row.contains('\u{2570}') {
            break;
        }
        for (col, ch) in row.char_indices() {
            if col < tl || col > tr {
                // Allow the canvas's own outer border column and pure whitespace.
                if ch != ' ' && ch != '\u{2502}' {
                    // Tolerate only the outer "mermaid (diagram)" panel border.
                    assert!(
                        col <= 2 || row.matches('\u{2502}').count() >= 2,
                        "row {row_off} col {col} leaks glyph `{ch}` outside composite bounds [.., {tl}..{tr}]:\n{row}"
                    );
                }
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state::tests::composite_inner_clipped_to_bounds`
Expected: FAIL — without clipping, the wide inner canvas stamps past the composite's right border.

- [ ] **Step 3: Write minimal implementation**

Add `stamp_canvas_clipped` to `src/diagram/canvas.rs` immediately after `stamp_canvas` (after line 461):

```rust
    /// Like `stamp_canvas`, but refuses to write any cell outside the
    /// rectangle `[dx, dx + max_w) × [dy, dy + max_h)`. Used to embed a
    /// composite state's inner canvas with a hard bound so an oversize
    /// inner render cannot corrupt neighbouring layers.
    pub(crate) fn stamp_canvas_clipped(
        &mut self,
        other: &Canvas,
        dx: usize,
        dy: usize,
        max_w: usize,
        max_h: usize,
    ) {
        let x_limit = max_w.min(other.width);
        let y_limit = max_h.min(other.height);
        for y in 0..y_limit {
            if dy + y >= self.height {
                break;
            }
            for x in 0..x_limit {
                if dx + x >= self.width {
                    break;
                }
                let src = &other.cells[y][x];
                if src.ch == ' '
                    && src.fg.is_none()
                    && src.bg.is_none()
                    && !src.is_node
                    && src.connects == 0
                {
                    continue;
                }
                self.cells[dy + y][dx + x] = src.clone();
            }
        }
    }
```

Use it at the composite stamp site in `src/diagram/graph/state.rs:572`. Replace:

```rust
                    canvas.stamp_canvas(inner_canvas, left_x + 2, node_y + 2);
```

with:

```rust
                    canvas.stamp_canvas_clipped(
                        inner_canvas,
                        left_x + 2,
                        node_y + 2,
                        w.saturating_sub(4),
                        h.saturating_sub(3),
                    );
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state::tests`
Expected: PASS — the clipping test and all others green.

- [ ] **Step 5: Commit**

```bash
git add src/diagram/canvas.rs src/diagram/graph/state.rs
git commit -m "fix(mermaid/state): clip composite inner stamps to interior bounds

Add Canvas::stamp_canvas_clipped and use it for composite-state inner
stamps. An inner render that would overflow its reserved interior is
clipped rather than corrupting neighbouring layers — bounds the
failure mode to 'composite looks cropped', never 'next layer clobbered'."
```

---

### Task 7: Integration test + full verification + manual smoke

**Files:**
- Modify: `src/diagram/graph/state.rs` `mod tests` (add the integration test)

**Interfaces:** none new.

- [ ] **Step 1: Write the integration test**

Add to `src/diagram/graph/state.rs` `mod tests`:

```rust
#[test]
fn demo_renders_cleanly() {
    // The exact mermaid block from demo/state-diagram.md.
    let src = "stateDiagram-v2
    [*] --> Created : new order
    Created --> Paid : payment_ok
    Created --> Cancelled : user_cancel

    Paid --> Packed : warehouse_pick
    Packed --> Shipped : label_printed
    Shipped --> Delivered : carrier_dropoff

    Delivered --> Closed : confirm
    Closed --> [*]
    Cancelled --> [*]

    note right of Paid
        Funds captured
        by the gateway
    end note

    state Cancelled {
        [*] --> Refunded
        Refunded --> [*]
    }";
    let rows = render_to_text(src);
    let all: String = rows.join("\n");

    // (a) All eight state names appear.
    for state in ["Created", "Paid", "Packed", "Shipped", "Delivered", "Closed", "Cancelled", "Refunded"] {
        assert!(all.contains(state), "state `{state}` missing, got:\n{all}");
    }
    // (b) All seven transition labels appear as contiguous substrings.
    for label in ["new order", "payment_ok", "user_cancel", "warehouse_pick", "label_printed", "carrier_dropoff", "confirm"] {
        assert!(
            all.contains(label),
            "transition label `{label}` must be contiguous, got:\n{all}"
        );
    }
    // (c) `end` does not appear as a standalone token.
    assert!(
        !all.split_whitespace().any(|t| t == "end"),
        "`end` keyword leaked into render, got:\n{all}"
    );
    // (d) Every emitted row's length matches the reported canvas width
    //     (catches out-of-bounds writes that the bounds-checked setters
    //     would otherwise silently drop).
    let theme = Theme::dark();
    let (span_rows, width) = render_mermaid(src, &theme).expect("rendered");
    for (i, row) in span_rows.iter().enumerate() {
        let row_len: usize = row.iter().map(|s| s.text.chars().count()).sum();
        assert!(
            row_len <= width,
            "row {i} length {row_len} exceeds canvas width {width}"
        );
    }
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --lib state::tests::demo_renders_cleanly`
Expected: PASS — all four assertion groups hold.

- [ ] **Step 3: Run full verification**

Run each, confirm clean:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo build --release
```

Expected: all four commands exit 0; no warnings.

- [ ] **Step 4: Manual smoke**

Reproduce the bug-report pipeline and compare to the baseline artefacts in `/tmp/opencode/`:

```bash
COLUMNS=120 ./target/release/mdterm demo/state-diagram.md > /tmp/opencode/mdterm_render_after.txt
diff /tmp/opencode/mdterm_render.txt /tmp/opencode/mdterm_render_after.txt | head -40
```

Expected: diff shows the note now reads `Funds captured / by the gateway` beside `Paid`, no stray `Funds`/`by`/`end` boxes at the top, all transition labels contiguous, and the `Cancelled` composite's outer rectangle fully contains its inner content.

- [ ] **Step 5: Commit**

```bash
git add src/diagram/graph/state.rs
git commit -m "test(mermaid/state): add demo integration regression test

Renders the demo/state-diagram.md mermaid block end-to-end and asserts
all 8 state names, all 7 transition labels contiguous, no `end` leak,
and no row exceeds the canvas width."
```

- [ ] **Step 6: Optional squash**

If the user wants a single clean commit on merge, squash tasks 1–7:

```bash
# Only if the user confirms they want a single commit.
git rebase -i HEAD~7
# pick task 1, squash tasks 2-7, edit message to:
# fix(mermaid): state-diagram notes, composite layout, edge-label reservation
```

---

## Self-Review Notes

- **Spec coverage:** All three bugs map to tasks: bug 1 → Tasks 1+2; bug 3 → Tasks 3+4; bug 2 → Tasks 5+6. Task 7 ties them together with the demo integration test from the spec's test plan.
- **Placeholder scan:** No TBD/TODO/vague-text in any step; every code step shows the actual code.
- **Type consistency:** `draw_note_card(left_x, top_y, width, height, lines: &[&str], border_fg, text_fg)` signature used identically in Task 2's definition and call site. `stamp_canvas_clipped(other, dx, dy, max_w, max_h)` identical in Task 6's definition and call site. `layer_down_edge_label_max(diagram, layers, layer_idx) -> usize` shared by Tasks 3 and 4.
- **Known soft spots:**
  - Task 4's test may already pass against the unmodified code (the minimal case happens to fit in 4 rows). It's kept as a regression guard. If the user prefers, the assertion can be strengthened to count blank rows.
  - Task 5's edge-loop change is a near-no-op today because `NodeLayout.top_y` already points at the composite border; the explicit form is groundwork. If the reviewer disagrees, the loop change can be dropped from Task 5 without affecting the size-reservation fix in the same task.
