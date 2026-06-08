# Mermaid LR Layout Optimizer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve LR Mermaid readability by refining node order and vertical spacing before routing edges.

**Architecture:** Keep the current `src/diagram.rs` parser, graph model, canvas, per-edge colors, rounded paths, and lane router. Add conservative helper functions for crossing-aware layer refinement and adaptive vertical placement, then call them from `render_lr()`.

**Tech Stack:** Rust 2024, existing `HashMap`/`Vec` helpers, existing `cargo` verification, existing terminal Mermaid renderer.

---

### Task 1: Add Crossing-Aware Layer Refinement

**Files:**
- Modify: `src/diagram.rs`

- [ ] **Step 1: Add a focused unit test for adjacent-layer crossing scoring**

Add this test inside the existing `#[cfg(test)] mod tests` in `src/diagram.rs`:

```rust
#[test]
fn adjacent_layer_crossing_score_counts_order_inversions() {
    let graph = parse_mermaid("graph LR\nA --> D\nB --> C").expect("expected graph");
    let crossing_layers = vec![
        vec!["A".to_string(), "B".to_string()],
        vec!["C".to_string(), "D".to_string()],
    ];
    let aligned_layers = vec![
        vec!["A".to_string(), "B".to_string()],
        vec!["D".to_string(), "C".to_string()],
    ];

    assert_eq!(adjacent_layer_crossing_score(&crossing_layers, &graph, 0), 1);
    assert_eq!(adjacent_layer_crossing_score(&aligned_layers, &graph, 0), 0);
}
```

- [ ] **Step 2: Run the targeted test and verify it fails**

Run:

```bash
cargo test adjacent_layer_crossing_score_counts_order_inversions
```

Expected: compile failure because `adjacent_layer_crossing_score` is not defined yet.

- [ ] **Step 3: Add crossing score helpers**

Add these helpers after `order_within_layers()` in `src/diagram.rs`:

```rust
fn adjacent_layer_crossing_score(layers: &[Vec<String>], graph: &Graph, left_layer: usize) -> usize {
    let Some(left) = layers.get(left_layer) else {
        return 0;
    };
    let Some(right) = layers.get(left_layer + 1) else {
        return 0;
    };

    let left_positions: HashMap<&str, usize> = left
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.as_str(), idx))
        .collect();
    let right_positions: HashMap<&str, usize> = right
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.as_str(), idx))
        .collect();

    let mut layer_edges: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            Some((
                *left_positions.get(edge.from.as_str())?,
                *right_positions.get(edge.to.as_str())?,
            ))
        })
        .collect();

    layer_edges.sort_unstable_by_key(|&(from, to)| (from, to));

    let mut crossings = 0;
    for i in 0..layer_edges.len() {
        for j in (i + 1)..layer_edges.len() {
            if layer_edges[i].0 < layer_edges[j].0 && layer_edges[i].1 > layer_edges[j].1 {
                crossings += 1;
            }
        }
    }
    crossings
}

fn total_lr_crossing_score(layers: &[Vec<String>], graph: &Graph) -> usize {
    (0..layers.len().saturating_sub(1))
        .map(|idx| adjacent_layer_crossing_score(layers, graph, idx))
        .sum()
}
```

- [ ] **Step 4: Add local adjacent-swap refinement**

Add this helper after the score helpers:

```rust
fn refine_lr_layer_order(layers: &mut [Vec<String>], graph: &Graph) {
    if layers.len() < 2 {
        return;
    }

    for _ in 0..4 {
        let mut improved = false;

        for layer_idx in 0..layers.len() {
            if layers[layer_idx].len() < 2 {
                continue;
            }

            let mut pos = 0;
            while pos + 1 < layers[layer_idx].len() {
                let before = total_lr_crossing_score(layers, graph);
                layers[layer_idx].swap(pos, pos + 1);
                let after = total_lr_crossing_score(layers, graph);

                if after < before {
                    improved = true;
                    pos += 1;
                } else {
                    layers[layer_idx].swap(pos, pos + 1);
                }

                pos += 1;
            }
        }

        if !improved {
            break;
        }
    }
}
```

- [ ] **Step 5: Call refinement from `render_lr()`**

Change this block:

```rust
let mut layers = assign_layers(graph);
order_within_layers(&mut layers, graph);
```

to:

```rust
let mut layers = assign_layers(graph);
order_within_layers(&mut layers, graph);
refine_lr_layer_order(&mut layers, graph);
```

- [ ] **Step 6: Run targeted and full verification**

Run:

```bash
cargo test adjacent_layer_crossing_score_counts_order_inversions
cargo fmt --check
cargo check
```

Expected: all commands pass.

### Task 2: Add Adaptive Vertical Spacing

**Files:**
- Modify: `src/diagram.rs`

- [ ] **Step 1: Add node spacing helper**

Add this helper near the LR layout helpers in `src/diagram.rs`:

```rust
fn lr_node_extra_gap(graph: &Graph, node_id: &str) -> usize {
    let degree = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node_id || edge.to == node_id)
        .count();

    match degree {
        0 | 1 => 0,
        2 | 3 => 1,
        _ => 2,
    }
}

fn lr_layer_height(layer: &[String], graph: &Graph, node_height: usize, base_gap: usize) -> usize {
    if layer.is_empty() {
        return 0;
    }

    let nodes_height = layer.len() * node_height;
    let gaps_height = layer
        .windows(2)
        .map(|pair| {
            let left_gap = lr_node_extra_gap(graph, &pair[0]);
            let right_gap = lr_node_extra_gap(graph, &pair[1]);
            base_gap + left_gap.max(right_gap)
        })
        .sum::<usize>();

    nodes_height + gaps_height
}
```

- [ ] **Step 2: Replace canvas height calculation**

Change this block in `render_lr()`:

```rust
let max_nodes_in_layer = layers.iter().map(|l| l.len()).max().unwrap_or(1);

let canvas_width: usize =
    col_widths.iter().sum::<usize>() + (layers.len().saturating_sub(1)) * node_h_gap + 4;
let canvas_height = max_nodes_in_layer * (node_height + v_gap) - v_gap + 2;
```

to:

```rust
let layer_heights: Vec<usize> = layers
    .iter()
    .map(|layer| lr_layer_height(layer, graph, node_height, v_gap))
    .collect();
let max_layer_height = layer_heights.iter().copied().max().unwrap_or(node_height);

let canvas_width: usize =
    col_widths.iter().sum::<usize>() + (layers.len().saturating_sub(1)) * node_h_gap + 4;
let canvas_height = max_layer_height + 2;
```

- [ ] **Step 3: Use adaptive gaps during node placement**

Change this block inside the layer loop in `render_lr()`:

```rust
let total_layer_height = layer.len() * node_height + layer.len().saturating_sub(1) * v_gap;
let start_y = (canvas_height.saturating_sub(total_layer_height)) / 2;

for (node_idx, id) in layer.iter().enumerate() {
    let w = widths.get(id).copied().unwrap_or(7);
    let cx = col_x + col_w / 2;
    let y = start_y + node_idx * (node_height + v_gap);
```

to:

```rust
let total_layer_height = layer_heights.get(layer_idx).copied().unwrap_or(node_height);
let start_y = (canvas_height.saturating_sub(total_layer_height)) / 2;
let mut y = start_y;

for (node_idx, id) in layer.iter().enumerate() {
    let w = widths.get(id).copied().unwrap_or(7);
    let cx = col_x + col_w / 2;
```

Then, before the end of the `for (node_idx, id)` loop, add:

```rust
if node_idx + 1 < layer.len() {
    let next_id = &layer[node_idx + 1];
    y += node_height + v_gap + lr_node_extra_gap(graph, id).max(lr_node_extra_gap(graph, next_id));
}
```

- [ ] **Step 4: Verify build and visual sample**

Run:

```bash
cargo fmt --check
cargo check
cargo build
```

Expected: all commands pass.

Then compare:

```bash
/tmp/mdterm-before/target/debug/mdterm /home/cle/Source/demo/mdterm/demo/mermaid-routing.md
./target/debug/mdterm demo/mermaid-routing.md
```

Expected: the after version has more vertical breathing room in dense LR areas and no worse obvious crossings than before.

### Task 3: Final Review and Commit

**Files:**
- Modify: `src/diagram.rs`
- Keep uncommitted unless intentionally included: `.superpowers/`
- Decide whether to include: `demo/mermaid-routing.md`
- Existing committed spec: `docs/superpowers/specs/2026-06-08-mermaid-lr-layout-optimizer-design.md`
- Plan file: `docs/superpowers/plans/2026-06-08-mermaid-lr-layout-optimizer.md`

- [ ] **Step 1: Inspect changed files**

Run:

```bash
git status --short
git diff --stat
```

Expected: renderer changes are in `src/diagram.rs`; plan/demo files are visible; `.superpowers/` remains local brainstorming output.

- [ ] **Step 2: Run final verification**

Run:

```bash
cargo fmt --check
cargo check
cargo test
cargo build
git diff --check
```

Expected: all commands pass.

- [ ] **Step 3: Commit implementation files**

Commit only the durable project files:

```bash
git add src/diagram.rs demo/mermaid-routing.md docs/superpowers/plans/2026-06-08-mermaid-lr-layout-optimizer.md
git commit -m "feat(mermaid): optimize lr diagram layout"
```

Do not commit `.superpowers/brainstorm/`.
