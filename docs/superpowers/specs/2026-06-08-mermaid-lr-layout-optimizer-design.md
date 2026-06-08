# Mermaid LR Layout Optimizer Design

## Goal

Improve left-to-right Mermaid diagram readability with a POC-sized layout-first
pass. The optimizer should reduce obvious crossings and crowding before edge
routing, without replacing the current parser, canvas, or renderer pipeline.

## Scope

In scope:

- LR Mermaid diagrams rendered through `src/diagram.rs`.
- Node ordering inside existing layers.
- Adaptive vertical placement for dense layer pairs.
- Existing per-edge colors, horizontal panning, no-wrap Mermaid behavior, and
  rounded edge drawing.

Out of scope:

- A full Sugiyama or Graphviz-style layout engine.
- New Mermaid syntax support.
- TD diagram changes.
- External graph layout dependencies.

## Architecture

Keep the current rendering flow:

1. Parse Mermaid into `Graph`.
2. Assign layers with `assign_layers()`.
3. Optimize node order and vertical spacing for LR diagrams.
4. Draw nodes on `Canvas`.
5. Route edges with the current lane-aware LR edge drawing.

The new optimizer sits between layer assignment and canvas placement. It should
use the existing `Graph`, layer vectors, and edge list, so the rest of the code
does not need a second graph representation.

## Components

### Crossing-Aware Ordering

The existing barycenter heuristic remains the first ordering pass. After it,
run a small local refinement pass that tries adjacent swaps inside each layer.
Keep a swap only when it lowers the crossing score for edges between neighboring
layer pairs.

The crossing score should count pairwise inversions between two adjacent layers:
if two edges preserve opposite source/destination order, they cross. This gives
a simple, deterministic signal without needing full geometry.

Tie handling should be stable. If a swap does not improve the score, preserve the
current order so diagrams do not churn between renders.

### Adaptive Vertical Spacing

Keep the same layer model, but compute node y positions with additional spacing
for dense or crossing-prone layer pairs. The spacing should be modest and
bounded, because terminal height matters.

Recommended POC rule:

- Start from the existing `v_gap`.
- For a layer, inspect edges connected to nodes in that layer and neighboring
  layers.
- Add one extra row of gap around nodes with multiple incoming or outgoing LR
  edges.
- Cap the extra gap so large diagrams grow gradually instead of exploding.

This gives crowded fan-out and fan-in areas more breathing room before the
current lane router draws the edges.

### Routing Integration

Keep the current lane-aware LR routing. The optimizer should improve the inputs
to the router:

- better vertical node order,
- more space between busy nodes,
- fewer obvious crossings for the same layer pair.

The router can continue to color each edge and use rounded corner glyphs. The
POC does not need to introduce dummy nodes or a separate edge-routing graph.

## Data Flow

`render_lr()` should become:

1. `assign_layers(graph)`.
2. `order_within_layers(&mut layers, graph)`.
3. `refine_lr_layer_order(&mut layers, graph)`.
4. Compute column widths.
5. Compute adaptive y positions for each layer.
6. Draw nodes.
7. Compute lane counts and route edges.

If a graph is too sparse for the optimizer to help, steps 3 and 5 should return
the current behavior.

## Error Handling

The optimizer should be conservative:

- Missing layer positions should fall back to the current order.
- Cycles continue using the existing cycle fallback from `assign_layers()`.
- Any local ordering candidate that does not strictly improve score is rejected.
- TD diagrams bypass this optimizer entirely.

## Verification

This is POC-sized, so implementation can prioritize build and visual checks over
new test coverage unless the code becomes risky.

Run:

- `cargo fmt --check`
- `cargo check`
- `cargo build`

Manual comparison:

- Before: `/tmp/mdterm-before/target/debug/mdterm demo/mermaid-routing.md`
- After: `./target/debug/mdterm demo/mermaid-routing.md`

Success criteria:

- `demo/mermaid-routing.md` shows fewer obvious LR crossings or cramped routes.
- Existing Mermaid diagrams still render.
- TD diagrams are unchanged.
- Wide diagrams remain horizontally pannable.
