use std::collections::HashMap;

use crate::style::StyledSpan;
use crate::theme::Theme;

use super::super::canvas::{Canvas, EdgeEnd, EdgeStyle, NodeShape};
use super::super::theme::edge_color;
use super::flowchart::{Direction, Edge, Graph, Node, parse_arrow};
use super::{NodeLayout, assign_layers, label_box_width, order_within_layers};

// Internal ids for the [*] pseudo-state. Each [*] reference collapses to
// either the initial or the final pseudo-state depending on which side of the
// transition it appears on.
const INITIAL_ID: &str = "__initial__";
const FINAL_ID: &str = "__final__";

// Private marker label rendered inside initial/final circles.
const DOT_GLYPH: &str = "\u{25c9}";

// ---- Data model ----

#[derive(Debug, Clone, Copy, PartialEq)]
enum StateKind {
    Normal,
    Initial,
    Final,
    Fork,
    Join,
}

#[derive(Debug, Clone)]
struct StateNode {
    label: String,
    kind: StateKind,
}

impl StateNode {
    fn shape(&self) -> NodeShape {
        match self.kind {
            StateKind::Normal => NodeShape::Rounded,
            StateKind::Initial => NodeShape::Circle,
            StateKind::Final => NodeShape::Final,
            StateKind::Fork | StateKind::Join => NodeShape::ForkBar,
        }
    }

    fn display_label(&self) -> String {
        match self.kind {
            StateKind::Initial | StateKind::Final => DOT_GLYPH.to_string(),
            _ => self.label.clone(),
        }
    }

    fn box_width(&self, edges: &[StateEdge], id: &str) -> usize {
        match self.kind {
            StateKind::Initial | StateKind::Final => 3,
            StateKind::Fork | StateKind::Join => {
                let degree = edges.iter().filter(|e| e.from == id || e.to == id).count();
                (degree * 3 + 4).clamp(5, 9)
            }
            StateKind::Normal => label_box_width(&self.label, NodeShape::Rounded),
        }
    }
}

#[derive(Debug, Clone)]
struct StateEdge {
    from: String,
    to: String,
    label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NoteSide {
    Left,
    Right,
    Over,
}

#[derive(Debug, Clone)]
struct StateNote {
    target: String,
    side: NoteSide,
    text: String,
}

#[derive(Debug, Clone)]
struct CompositeBody {
    title: String,
    source: String,
}

struct StateDiagram {
    nodes: HashMap<String, StateNode>,
    edges: Vec<StateEdge>,
    notes: Vec<StateNote>,
    composites: HashMap<String, CompositeBody>,
    node_order: Vec<String>,
}

// ---- Parser ----

fn register_state(
    nodes: &mut HashMap<String, StateNode>,
    node_order: &mut Vec<String>,
    id: &str,
    label: &str,
    kind: StateKind,
) {
    if let Some(node) = nodes.get_mut(id) {
        // Promotion: a Normal placeholder may be upgraded to a more specific
        // kind when the [*]/<<fork>>/<<join>> declaration is encountered.
        if kind != StateKind::Normal && node.kind == StateKind::Normal {
            node.kind = kind;
        }
        // Don't overwrite a meaningful label with an empty alias.
        if !label.is_empty() && label != id {
            node.label = label.to_string();
        }
    } else {
        let resolved_label = if label.is_empty() {
            id.to_string()
        } else {
            label.to_string()
        };
        nodes.insert(
            id.to_string(),
            StateNode {
                label: resolved_label,
                kind,
            },
        );
        node_order.push(id.to_string());
    }
}

/// Extract a `<<stereotype>>` token from the rest of a `state ...` line.
fn extract_stereotype(s: &str) -> Option<&str> {
    let start = s.find("<<")?;
    let end = s[start..].find(">>").map(|e| e + start)?;
    Some(&s[start + 2..end])
}

/// Parse `"Long Label" as Id` or `Id` into (title, id).
fn parse_alias_decl(s: &str) -> (String, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"')
        && let Some(end) = rest.find('"')
        && let Some(id) = rest[end + 1..].trim().strip_prefix("as ")
    {
        let label = rest[..end].to_string();
        return (label, id.trim().to_string());
    }
    let id = s.split_whitespace().next().unwrap_or(s);
    (id.to_string(), id.to_string())
}

/// Read the body of a composite state, tracking brace depth. Returns the
/// synthetic source (with a `stateDiagram-v2` header prepended) and the index
/// of the next unread source line.
fn collect_block_body(lines: &[&str], start: usize) -> (String, usize) {
    let mut inner_lines: Vec<String> = vec!["stateDiagram-v2".to_string()];
    let mut depth: usize = 1;
    let mut i = start;
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        let lt = l.trim();
        if lt == "}" {
            depth -= 1;
            if depth == 0 {
                break;
            }
            inner_lines.push(l.to_string());
            continue;
        }
        let opens = lt.matches('{').count();
        let closes = lt.matches('}').count();
        depth += opens;
        depth = depth.saturating_sub(closes);
        if depth == 0 {
            inner_lines.push(l.to_string());
            break;
        }
        inner_lines.push(l.to_string());
    }
    (inner_lines.join("\n"), i)
}

/// Parse a single state identifier (or `[*]`) from the start of `s`.
/// Returns the id and the rest of the string after it.
fn parse_state_id(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix("[*]") {
        return Some(("[*]".to_string(), rest));
    }
    let end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s[..end].to_string(), &s[end..]))
}

/// Parse `A --> B : label` (and `[*]` variants). Both endpoints are returned
/// with canonical ids (`__initial__`/`__final__` for `[*]`).
fn parse_state_transition(line: &str) -> Option<StateEdge> {
    let (from_raw, rest) = parse_state_id(line)?;
    let rest = rest.trim_start();
    let (_arrow_label, after_arrow) = parse_arrow(rest)?;
    let (to_raw, after_to) = parse_state_id(after_arrow.trim_start())?;

    let edge_label = after_to
        .trim_start()
        .strip_prefix(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let from = if from_raw == "[*]" {
        INITIAL_ID.to_string()
    } else {
        from_raw
    };
    let to = if to_raw == "[*]" {
        FINAL_ID.to_string()
    } else {
        to_raw
    };

    Some(StateEdge {
        from,
        to,
        label: edge_label,
    })
}

fn parse_state_note(line: &str) -> Option<StateNote> {
    let rest = line["note".len()..].trim_start();
    let (side, target_str) = if let Some(r) = rest.strip_prefix("left of ") {
        (NoteSide::Left, r)
    } else if let Some(r) = rest.strip_prefix("right of ") {
        (NoteSide::Right, r)
    } else if let Some(r) = rest.strip_prefix("over ") {
        (NoteSide::Over, r)
    } else {
        return None;
    };
    let (target, text) = match target_str.split_once(':') {
        Some((t, txt)) => (t.trim(), txt.trim().to_string()),
        None => (target_str.trim(), String::new()),
    };
    if target.is_empty() {
        return None;
    }
    Some(StateNote {
        target: target.to_string(),
        side,
        text,
    })
}

fn parse_state_diagram(code: &str) -> Option<StateDiagram> {
    let mut nodes: HashMap<String, StateNode> = HashMap::new();
    let mut edges: Vec<StateEdge> = Vec::new();
    let mut notes: Vec<StateNote> = Vec::new();
    let mut composites: HashMap<String, CompositeBody> = HashMap::new();
    let mut node_order: Vec<String> = Vec::new();

    let lines: Vec<&str> = code.lines().collect();
    let mut i = 0usize;

    // Locate the header line.
    let mut found_header = false;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() || t.starts_with("%%") {
            i += 1;
            continue;
        }
        if t == "stateDiagram" || t == "stateDiagram-v2" {
            i += 1;
            found_header = true;
            break;
        }
        return None;
    }
    if !found_header {
        return None;
    }

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();
        i += 1;

        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        // Skip directives we don't model.
        if trimmed.starts_with("skinparam")
            || trimmed.starts_with("direction")
            || trimmed.starts_with("scale")
            || trimmed.starts_with("style ")
            || trimmed.starts_with("classDef ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("linkStyle ")
            || trimmed.starts_with("link ")
            || trimmed == "}"
        {
            continue;
        }

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

        // `state ...` declarations.
        if let Some(rest) = trimmed.strip_prefix("state ") {
            // Fork/Join stereotypes: `state fork_state <<fork>>`.
            if let Some(stereotype) = extract_stereotype(rest) {
                let name_part = rest[..rest.find("<<").unwrap_or(rest.len())].trim();
                let id = name_part.trim_matches('"');
                let kind = match stereotype {
                    "fork" => StateKind::Fork,
                    "join" => StateKind::Join,
                    _ => StateKind::Normal,
                };
                if matches!(kind, StateKind::Fork | StateKind::Join) {
                    register_state(&mut nodes, &mut node_order, id, id, kind);
                    continue;
                }
            }

            // Composite state: `state Foo {` or `state "Title" as Foo {`.
            if trimmed.ends_with('{') {
                let title_part = rest[..rest.len() - 1].trim();
                let (title, id) = parse_alias_decl(title_part);
                let (body, advanced) = collect_block_body(&lines, i);
                i = advanced;
                register_state(&mut nodes, &mut node_order, &id, &title, StateKind::Normal);
                composites.insert(
                    id.clone(),
                    CompositeBody {
                        title,
                        source: body,
                    },
                );
                continue;
            }

            // Alias declaration: `state "Long Label" as Id`.
            if let Some(rest_quoted) = rest.strip_prefix('"')
                && let Some(end) = rest_quoted.find('"')
                && let Some(id_part) = rest_quoted[end + 1..].trim().strip_prefix("as ")
            {
                let label = rest_quoted[..end].to_string();
                let id = id_part.trim().to_string();
                register_state(&mut nodes, &mut node_order, &id, &label, StateKind::Normal);
                continue;
            }

            // Unknown `state ...` directive; skip.
            continue;
        }

        // Transitions.
        if let Some(edge) = parse_state_transition(trimmed) {
            let from_kind = if edge.from == INITIAL_ID {
                StateKind::Initial
            } else {
                StateKind::Normal
            };
            let to_kind = if edge.to == FINAL_ID {
                StateKind::Final
            } else {
                StateKind::Normal
            };
            register_state(&mut nodes, &mut node_order, &edge.from, "", from_kind);
            register_state(&mut nodes, &mut node_order, &edge.to, "", to_kind);
            edges.push(edge);
            continue;
        }

        // Bare state declaration: `Idle` on its own line.
        let id = trimmed.split_whitespace().next().unwrap_or(trimmed);
        if !id.is_empty() {
            register_state(&mut nodes, &mut node_order, id, id, StateKind::Normal);
        }
    }

    if nodes.is_empty() {
        return None;
    }

    Some(StateDiagram {
        nodes,
        edges,
        notes,
        composites,
        node_order,
    })
}

// ---- Renderer ----

fn to_graph(diagram: &StateDiagram) -> Graph {
    let nodes: HashMap<String, Node> = diagram
        .nodes
        .iter()
        .map(|(id, sn)| {
            (
                id.clone(),
                Node {
                    label: sn.label.clone(),
                    shape: sn.shape(),
                },
            )
        })
        .collect();
    let edges: Vec<Edge> = diagram
        .edges
        .iter()
        .map(|e| Edge {
            from: e.from.clone(),
            to: e.to.clone(),
            label: e.label.clone(),
        })
        .collect();
    Graph {
        direction: Direction::TopDown,
        nodes,
        edges,
        node_order: diagram.node_order.clone(),
    }
}

/// Compute left/right/over canvas padding required by the diagram's notes.
/// Returns `(left_pad, right_pad, over_top_pad)`. Each side pad is the max
/// note width on that side plus a 2-column gap; `over_top_pad` is the max
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
        let w = longest.max(3) + 4 + 2; // box width + gap
        let h = lines.len() + 2 + 1; // box height + gap row
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

/// Longest `chars().count()` among edge labels whose `from` node sits in
/// `layers[layer_idx]` and whose `to` node sits in the next layer
/// (`layers[layer_idx + 1]`). Used to reserve horizontal room so down-edge
/// labels drawn rightward from the arrow never reach a same-layer
/// neighbour's box.
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

#[allow(clippy::too_many_lines)]
fn render_state_canvas(diagram: &StateDiagram, theme: &Theme) -> Option<(Canvas, usize)> {
    let graph = to_graph(diagram);
    let mut layers = assign_layers(&graph);
    order_within_layers(&mut layers, &graph);

    // Compute widths/heights, expanding composites by recursively rendering
    // their inner graphs.
    let mut widths: HashMap<String, usize> = HashMap::new();
    let mut heights: HashMap<String, usize> = HashMap::new();
    let mut subcanvases: HashMap<String, Canvas> = HashMap::new();

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

    let h_gap_floor = 4;
    let edge_gap = 4;
    let base_height = 3;

    let layer_h_gaps: Vec<usize> = (0..layers.len())
        .map(|i| {
            layer_down_edge_label_max(diagram, &layers, i)
                .saturating_add(2)
                .max(h_gap_floor)
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

    let layer_heights: Vec<usize> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|id| heights.get(id).copied().unwrap_or(base_height))
                .max()
                .unwrap_or(base_height)
        })
        .collect();

    let total_height: usize =
        layer_heights.iter().sum::<usize>() + layers.len().saturating_sub(1) * edge_gap;

    // Reserve side + top padding so notes have somewhere to live. Size from
    // the actual note boxes (longest line + 4 wide, n_lines + 2 tall) rather
    // than a flat guess.
    let (left_pad, right_pad, over_pad) = note_padding(&diagram.notes);
    let side_padding = left_pad.max(right_pad).max(2);
    let top_padding = over_pad;

    let canvas_width = max_layer_width + side_padding * 2;
    let canvas_height = total_height + top_padding;
    if canvas_height == 0 {
        return None;
    }

    let mut canvas = Canvas::new(canvas_width, canvas_height);
    let border_fg = Some(theme.code_border);
    let text_fg = Some(theme.fg);
    let comp_bg = Some(theme.composite_state_bg);

    let mut positions: HashMap<String, NodeLayout> = HashMap::new();
    let canvas_center = canvas_width / 2;

    let mut y = top_padding;
    for (layer_idx, layer) in layers.iter().enumerate() {
        let layer_height = layer_heights[layer_idx];
        let h_gap = layer_h_gaps[layer_idx];

        let node_widths: Vec<usize> = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .collect();
        let layer_width: usize =
            node_widths.iter().sum::<usize>() + layer.len().saturating_sub(1) * h_gap;
        let layer_center = if layer_width > 0 { layer_width / 2 } else { 0 };

        let mut centers: Vec<usize> = Vec::with_capacity(layer.len());
        let mut cum = 0usize;
        for &w in &node_widths {
            centers.push(cum + w / 2);
            cum += w + h_gap;
        }

        for (i, id) in layer.iter().enumerate() {
            let w = node_widths[i];
            let h = heights.get(id).copied().unwrap_or(base_height);
            let cx = (canvas_center as isize + centers[i] as isize - layer_center as isize)
                .max(w as isize / 2) as usize;
            let node_y = y + (layer_height - h) / 2;

            if let Some(sn) = diagram.nodes.get(id) {
                if let Some(inner_canvas) = subcanvases.get(id) {
                    let left_x = cx.saturating_sub(w / 2);
                    canvas.draw_composite_outer(
                        left_x,
                        node_y,
                        w,
                        h,
                        &diagram.composites[id].title,
                        border_fg,
                        text_fg,
                        comp_bg,
                    );
                    canvas.stamp_canvas(inner_canvas, left_x + 2, node_y + 2);
                } else {
                    let label = sn.display_label();
                    let shape = sn.shape();
                    canvas.draw_node(cx, node_y, w, &label, shape, border_fg, text_fg);
                }
            }

            positions.insert(
                id.clone(),
                NodeLayout {
                    center_x: cx,
                    top_y: node_y,
                    width: w,
                    height: h,
                },
            );
        }

        y += layer_height + edge_gap;
    }

    // Edges.
    for (edge_idx, edge) in diagram.edges.iter().enumerate() {
        if let (Some(src), Some(dst)) = (positions.get(&edge.from), positions.get(&edge.to)) {
            let src_bottom = src.top_y + src.height;
            let dst_top = dst.top_y;
            let edge_fg = Some(edge_color(theme, edge_idx));
            canvas.draw_edge_td(
                src.center_x,
                src_bottom,
                dst.center_x,
                dst_top,
                EdgeStyle {
                    head: EdgeEnd::Arrow,
                    label: edge.label.as_deref(),
                    ..Default::default()
                },
                edge_fg,
            );
        }
    }

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
                    let left_x = target
                        .center_x
                        .saturating_sub(target.width / 2 + note_w + 2);
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

    Some((canvas, canvas_width))
}

pub(crate) fn render(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let diagram = parse_state_diagram(code)?;
    let (canvas, width) = render_state_canvas(&diagram, theme)?;
    Some((canvas.to_span_rows(theme), width))
}

#[cfg(test)]
mod tests {
    use super::super::super::render_mermaid;
    use super::*;
    use crate::style::StyledSpan;
    use crate::theme::Theme;

    fn row_text(row: &[StyledSpan]) -> String {
        row.iter().map(|span| span.text.as_str()).collect()
    }

    fn render_to_text(code: &str) -> Vec<String> {
        let theme = Theme::dark();
        let (rows, _w) = render_mermaid(code, &theme).expect("expected rendered diagram");
        rows.iter().map(|r| row_text(r)).collect()
    }

    // ---- Parser unit tests ----

    #[test]
    fn parses_simple_states_and_transition() {
        let d = parse_state_diagram("stateDiagram-v2\nA --> B : go").unwrap();
        assert_eq!(d.nodes.len(), 2);
        assert!(d.nodes.contains_key("A"));
        assert!(d.nodes.contains_key("B"));
        assert_eq!(d.edges.len(), 1);
        let e = &d.edges[0];
        assert_eq!(e.from, "A");
        assert_eq!(e.to, "B");
        assert_eq!(e.label.as_deref(), Some("go"));
    }

    #[test]
    fn parses_initial_and_final_pseudo_states() {
        let d = parse_state_diagram("stateDiagram-v2\n[*] --> Idle\nIdle --> [*]").unwrap();
        assert!(d.nodes.contains_key(INITIAL_ID));
        assert!(d.nodes.contains_key(FINAL_ID));
        assert!(d.nodes.contains_key("Idle"));
        assert_eq!(d.nodes[INITIAL_ID].kind, StateKind::Initial);
        assert_eq!(d.nodes[FINAL_ID].kind, StateKind::Final);
        assert_eq!(d.edges.len(), 2);
        assert_eq!(d.edges[0].from, INITIAL_ID);
        assert_eq!(d.edges[0].to, "Idle");
        assert_eq!(d.edges[1].from, "Idle");
        assert_eq!(d.edges[1].to, FINAL_ID);
    }

    #[test]
    fn parses_long_label_alias() {
        let d = parse_state_diagram("stateDiagram-v2\nstate \"Long Label\" as Foo\nFoo --> Bar")
            .unwrap();
        let foo = &d.nodes["Foo"];
        assert_eq!(foo.label, "Long Label");
        assert_eq!(foo.kind, StateKind::Normal);
    }

    #[test]
    fn parses_fork_and_join_stereotypes() {
        let d = parse_state_diagram(
            "stateDiagram-v2\nstate fork1 <<fork>>\nstate join1 <<join>>\nfork1 --> join1",
        )
        .unwrap();
        assert_eq!(d.nodes["fork1"].kind, StateKind::Fork);
        assert_eq!(d.nodes["join1"].kind, StateKind::Join);
    }

    #[test]
    fn parses_composite_state_body() {
        let src = "stateDiagram-v2\nstate Foo {\n  Inner1 --> Inner2 : step\n}";
        let d = parse_state_diagram(src).unwrap();
        let comp = d.composites.get("Foo").expect("composite should be parsed");
        assert_eq!(comp.title, "Foo");
        // Recursive parse should pick up the inner states.
        let inner = parse_state_diagram(&comp.source).unwrap();
        assert!(inner.nodes.contains_key("Inner1"));
        assert!(inner.nodes.contains_key("Inner2"));
        assert_eq!(inner.edges.len(), 1);
    }

    #[test]
    fn parses_note_placements() {
        let d = parse_state_diagram(
            "stateDiagram-v2\nA\nnote left of A : hello\nnote right of A : world",
        )
        .unwrap();
        assert_eq!(d.notes.len(), 2);
        assert_eq!(d.notes[0].side, NoteSide::Left);
        assert_eq!(d.notes[0].target, "A");
        assert_eq!(d.notes[0].text, "hello");
        assert_eq!(d.notes[1].side, NoteSide::Right);
        assert_eq!(d.notes[1].text, "world");
    }

    #[test]
    fn parses_multiline_note_block() {
        let src = "stateDiagram-v2\n[*] --> Paid\nnote right of Paid\n    Funds captured\n    by the gateway\nend note\nPaid --> [*]";
        let d = parse_state_diagram(src).unwrap();
        assert_eq!(d.notes.len(), 1, "exactly one note, got: {:?}", d.notes);
        assert_eq!(d.notes[0].target, "Paid");
        assert_eq!(d.notes[0].side, NoteSide::Right);
        assert_eq!(
            d.notes[0].text, "Funds captured\nby the gateway",
            "block body should join trimmed lines with \\n"
        );
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
        assert_eq!(
            d.edges.len(),
            2,
            "parser should resume after the note block"
        );
        assert!(d.nodes.contains_key("B"));
    }

    #[test]
    fn rejects_non_state_header() {
        assert!(parse_state_diagram("graph TD\nA --> B").is_none());
    }

    // ---- Renderer smoke tests ----

    #[test]
    fn renders_simple_transition() {
        let rows = render_to_text("stateDiagram-v2\n[*] --> Idle\nIdle --> [*]");
        let all: String = rows.join("\n");
        // Initial state glyph.
        assert!(
            all.contains('\u{25c9}'),
            "initial state should show \u{25c9}, got:\n{all}"
        );
        // Final state glyph (also \u{25c9} inside its ring).
        assert!(
            all.matches('\u{25c9}').count() >= 2,
            "expected at least two \u{25c9} (initial + final), got:\n{all}"
        );
        // The Idle label should appear.
        assert!(
            all.contains("Idle"),
            "Idle label should appear, got:\n{all}"
        );
    }

    #[test]
    fn renders_arrow_glyphs() {
        let rows =
            render_to_text("stateDiagram-v2\n[*] --> Idle\nIdle --> Active : go\nActive --> [*]");
        let all: String = rows.join("\n");
        // TD routing uses \u{25bc} (down-arrow) at the destination end.
        assert!(
            all.contains('\u{25bc}') || all.contains('\u{25b6}'),
            "expected a \u{25bc} or \u{25b6} arrow glyph, got:\n{all}"
        );
        // Transition label should appear.
        assert!(
            all.contains("go"),
            "transition label should appear, got:\n{all}"
        );
    }

    #[test]
    fn renders_fork_bar() {
        let rows = render_to_text(
            "stateDiagram-v2\nstate fork1 <<fork>>\nstate join1 <<join>>\nfork1 --> join1",
        );
        let all: String = rows.join("\n");
        // ForkBar renders as a run of \u{2550} (DOUBLE HORIZONTAL).
        assert!(
            all.contains('\u{2550}'),
            "fork/join should render as \u{2550} bar, got:\n{all}"
        );
    }

    #[test]
    fn renders_note_text() {
        let rows = render_to_text("stateDiagram-v2\n[*] --> Idle\nnote right of Idle : ping");
        let all: String = rows.join("\n");
        assert!(all.contains("ping"), "note text should appear, got:\n{all}");
    }

    #[test]
    fn renders_multiline_note_beside_target() {
        let rows = render_to_text(
            "stateDiagram-v2\n[*] --> Paid\nnote right of Paid\n    Funds captured\n    by the gateway\nend note\nPaid --> [*]",
        );
        let all: String = rows.join("\n");
        assert!(
            all.contains("Funds"),
            "first note line should appear, got:\n{all}"
        );
        assert!(
            all.contains("gateway"),
            "second note line should appear, got:\n{all}"
        );
        assert!(
            !all.split_whitespace().any(|t| t == "end"),
            "`end` keyword must not leak into render, got:\n{all}"
        );
    }

    #[test]
    fn renders_composite_state_label() {
        let rows = render_to_text(
            "stateDiagram-v2\n[*] --> Outer\nstate Outer {\n  Inner1 --> Inner2\n}\nOuter --> [*]",
        );
        let all: String = rows.join("\n");
        assert!(
            all.contains("Outer"),
            "composite state title should appear, got:\n{all}"
        );
        assert!(
            all.contains("Inner1"),
            "inner state label should appear via sub-canvas, got:\n{all}"
        );
    }

    #[test]
    fn canvas_dimensions_positive_and_label_present() {
        let theme = Theme::dark();
        let (rows, width) = render_mermaid(
            "stateDiagram-v2\n[*] --> Idle\nIdle --> Active : go\nActive --> [*]",
            &theme,
        )
        .expect("rendered");
        assert!(width > 0, "canvas width should be positive");
        assert!(!rows.is_empty(), "should produce at least one row");
        let all: String = rows
            .iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all.contains("Active"),
            "known state label should appear, got:\n{all}"
        );
    }

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
        assert!(all.contains('B'), "target state B missing, got:\n{all}");
        assert!(all.contains('X'), "sibling state X missing, got:\n{all}");
    }
}
