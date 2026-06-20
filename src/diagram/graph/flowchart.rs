use std::collections::HashMap;

use crate::style::StyledSpan;
use crate::theme::Theme;

use super::super::canvas::{Canvas, EdgeEnd, EdgeStyle, NodeShape};
use super::super::theme::edge_color;
use super::{
    NodeLayout, assign_layers, lr_edge_port_maps, lr_edge_port_y, lr_lane_counts, lr_lane_key,
    lr_lane_mid_x, lr_layer_height, lr_node_extra_gap, lr_node_height, node_box_width, node_left_x,
    node_right_x, order_within_layers, refine_lr_layer_order,
};

// ───── Data types ─────

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Direction {
    TopDown,
    LeftRight,
}

#[derive(Debug, Clone)]
pub(super) struct Node {
    pub(super) label: String,
    pub(super) shape: NodeShape,
}

#[derive(Debug, Clone)]
pub(super) struct Edge {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) label: Option<String>,
}

#[derive(Debug)]
pub(super) struct Graph {
    pub(super) direction: Direction,
    pub(super) nodes: HashMap<String, Node>,
    pub(super) edges: Vec<Edge>,
    pub(super) node_order: Vec<String>,
}

// ───── Parser ─────

fn parse_mermaid(code: &str) -> Option<Graph> {
    let mut direction = Direction::TopDown;
    let mut nodes: HashMap<String, Node> = HashMap::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut node_order: Vec<String> = Vec::new();

    for line in code.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        // Direction declaration
        if trimmed.starts_with("graph ") || trimmed.starts_with("flowchart ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                direction = match parts[1] {
                    "LR" | "RL" => Direction::LeftRight,
                    _ => Direction::TopDown,
                };
            }
            continue;
        }

        // Skip unsupported directives
        if trimmed.starts_with("subgraph")
            || trimmed == "end"
            || trimmed.starts_with("style ")
            || trimmed.starts_with("classDef ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("linkStyle ")
            || trimmed.starts_with("click ")
        {
            continue;
        }

        parse_line(trimmed, &mut nodes, &mut edges, &mut node_order);
    }

    if nodes.is_empty() {
        return None;
    }

    Some(Graph {
        direction,
        nodes,
        edges,
        node_order,
    })
}

#[allow(clippy::type_complexity)]
pub(super) fn parse_node_ref(s: &str) -> Option<(String, Option<(String, NodeShape)>, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }

    // Extract node ID (alphanumeric, underscore, hyphen)
    let id_end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(s.len());
    if id_end == 0 {
        return None;
    }
    let id = s[..id_end].to_string();
    let rest = &s[id_end..];

    // Double parens: ((label))
    if rest.starts_with("((")
        && let Some(end) = rest.find("))")
    {
        let label = rest[2..end].trim().to_string();
        return Some((id, Some((label, NodeShape::Circle)), &rest[end + 2..]));
    }

    // Square brackets: [label]
    if rest.starts_with('[')
        && let Some(end) = find_matching(rest, '[', ']')
    {
        let label = rest[1..end].trim().to_string();
        return Some((id, Some((label, NodeShape::Rectangle)), &rest[end + 1..]));
    }

    // Curly braces: {label}
    if rest.starts_with('{')
        && let Some(end) = find_matching(rest, '{', '}')
    {
        let label = rest[1..end].trim().to_string();
        return Some((id, Some((label, NodeShape::Diamond)), &rest[end + 1..]));
    }

    // Parentheses: (label)
    if rest.starts_with('(')
        && let Some(end) = find_matching(rest, '(', ')')
    {
        let label = rest[1..end].trim().to_string();
        return Some((id, Some((label, NodeShape::Rounded)), &rest[end + 1..]));
    }

    Some((id, None, rest))
}

fn find_matching(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth: usize = 0;
    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                continue;
            }
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn register_node(
    id: &str,
    label_shape: Option<(String, NodeShape)>,
    nodes: &mut HashMap<String, Node>,
    node_order: &mut Vec<String>,
) {
    if let Some(node) = nodes.get_mut(id) {
        if let Some((label, shape)) = label_shape {
            node.label = label;
            node.shape = shape;
        }
    } else {
        let (label, shape) = label_shape.unwrap_or_else(|| (id.to_string(), NodeShape::Rectangle));
        nodes.insert(id.to_string(), Node { label, shape });
        node_order.push(id.to_string());
    }
}

fn parse_line(
    line: &str,
    nodes: &mut HashMap<String, Node>,
    edges: &mut Vec<Edge>,
    node_order: &mut Vec<String>,
) {
    let (first_id, first_label, mut remaining) = match parse_node_ref(line) {
        Some(r) => r,
        None => return,
    };
    register_node(&first_id, first_label, nodes, node_order);

    let mut prev_id = first_id;

    // Parse chain of edges: A --> B --> C
    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() {
            break;
        }

        let (edge_label, arrow_rest) = match parse_arrow(trimmed) {
            Some(r) => r,
            None => break,
        };

        remaining = arrow_rest;

        let (next_id, next_label, rest) = match parse_node_ref(remaining) {
            Some(r) => r,
            None => break,
        };
        register_node(&next_id, next_label, nodes, node_order);

        edges.push(Edge {
            from: prev_id.clone(),
            to: next_id.clone(),
            label: edge_label,
        });

        prev_id = next_id;
        remaining = rest;
    }
}

pub(super) fn parse_arrow(s: &str) -> Option<(Option<String>, &str)> {
    let s = s.trim_start();

    // "-- label -->" syntax
    if s.starts_with("-- ")
        && let Some(arrow_pos) = s[3..].find("-->")
    {
        let label = s[3..3 + arrow_pos].trim().to_string();
        let rest = &s[3 + arrow_pos + 3..];
        return Some((Some(label), rest));
    }

    // Standard arrows
    let arrows = ["--->", "-->", "---", "-.->", "==>"];
    for arrow in &arrows {
        if let Some(rest) = s.strip_prefix(arrow) {
            // Check for |label| after arrow
            let trimmed_rest = rest.trim_start();
            if trimmed_rest.starts_with('|')
                && let Some(end) = trimmed_rest[1..].find('|')
            {
                let label = trimmed_rest[1..1 + end].trim().to_string();
                return Some((Some(label), &trimmed_rest[2 + end..]));
            }
            return Some((None, rest));
        }
    }

    None
}

// ───── Top-Down rendering ─────

fn render_td(graph: &Graph, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let node_height: usize = 3;
    let edge_gap: usize = 4;
    let h_gap: usize = 4;

    let mut layers = assign_layers(graph);
    order_within_layers(&mut layers, graph);

    // Calculate node widths
    let mut widths: HashMap<String, usize> = HashMap::new();
    for (id, node) in &graph.nodes {
        widths.insert(id.clone(), node_box_width(node));
    }

    // Find widest layer to determine canvas width
    let mut max_layer_width: usize = 0;
    for layer in &layers {
        let w: usize = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .sum::<usize>()
            + layer.len().saturating_sub(1) * h_gap;
        max_layer_width = max_layer_width.max(w);
    }

    let canvas_width = max_layer_width + 6; // margin on each side
    let canvas_height = layers.len() * (node_height + edge_gap) - edge_gap;

    if canvas_height == 0 {
        return None;
    }

    let mut canvas = Canvas::new(canvas_width, canvas_height);

    // Calculate node positions and draw nodes
    let mut positions: HashMap<String, NodeLayout> = HashMap::new();
    let border_fg = Some(theme.code_border);
    let text_fg = Some(theme.fg);

    // First pass: calculate centers for the widest layer
    // Then align single-node layers to the canvas center
    let canvas_center = canvas_width / 2;

    for (layer_idx, layer) in layers.iter().enumerate() {
        let y = layer_idx * (node_height + edge_gap);

        // Compute node centers relative to layer, then offset to center in canvas
        let node_widths_in_layer: Vec<usize> = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .collect();
        let layer_width: usize =
            node_widths_in_layer.iter().sum::<usize>() + layer.len().saturating_sub(1) * h_gap;

        // Compute center of each node within the layer
        let mut centers_in_layer: Vec<usize> = Vec::new();
        let mut cumulative = 0;
        for &w in &node_widths_in_layer {
            centers_in_layer.push(cumulative + w / 2);
            cumulative += w + h_gap;
        }

        // Center of the layer
        let layer_center = if layer_width > 0 { layer_width / 2 } else { 0 };

        for (i, id) in layer.iter().enumerate() {
            let w = node_widths_in_layer[i];
            // Shift node center so that the layer center aligns with canvas center
            let cx = (canvas_center as isize + centers_in_layer[i] as isize - layer_center as isize)
                .max(w as isize / 2) as usize;

            if let Some(node) = graph.nodes.get(id) {
                canvas.draw_node(cx, y, w, &node.label, node.shape, border_fg, text_fg);
            }

            positions.insert(
                id.clone(),
                NodeLayout {
                    center_x: cx,
                    top_y: y,
                    width: w,
                    height: node_height,
                },
            );
        }
    }

    // Draw edges
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if let (Some(src), Some(dst)) = (positions.get(&edge.from), positions.get(&edge.to)) {
            let src_bottom = src.top_y + 2;
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

    let rows = canvas.to_span_rows(theme);
    Some((rows, canvas_width))
}

// ───── Left-Right rendering ─────

fn render_lr(graph: &Graph, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let node_height: usize = 3;
    let node_h_gap: usize = 18; // horizontal gap between columns for edge routing
    let v_gap: usize = 3; // vertical gap between nodes in same column

    let mut layers = assign_layers(graph);
    order_within_layers(&mut layers, graph);
    refine_lr_layer_order(&mut layers, graph);

    // Calculate node widths
    let mut widths: HashMap<String, usize> = HashMap::new();
    for (id, node) in &graph.nodes {
        widths.insert(id.clone(), node_box_width(node));
    }
    let node_heights: HashMap<String, usize> = graph
        .nodes
        .keys()
        .map(|id| (id.clone(), lr_node_height(graph, id)))
        .collect();

    // Column widths (max node width per layer)
    let col_widths: Vec<usize> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|id| widths.get(id).copied().unwrap_or(7))
                .max()
                .unwrap_or(7)
        })
        .collect();

    let layer_heights: Vec<usize> = layers
        .iter()
        .map(|layer| lr_layer_height(layer, graph, &node_heights, v_gap))
        .collect();
    let max_layer_height = layer_heights.iter().copied().max().unwrap_or(node_height);

    let canvas_width: usize =
        col_widths.iter().sum::<usize>() + (layers.len().saturating_sub(1)) * node_h_gap + 4;
    let canvas_height = max_layer_height + 2;

    if canvas_height == 0 {
        return None;
    }

    let mut canvas = Canvas::new(canvas_width, canvas_height);

    let mut positions: HashMap<String, NodeLayout> = HashMap::new();
    let mut layer_by_id: HashMap<String, usize> = HashMap::new();
    let border_fg = Some(theme.code_border);
    let text_fg = Some(theme.fg);

    let mut col_x = 2; // starting x with margin
    for (layer_idx, layer) in layers.iter().enumerate() {
        let col_w = col_widths[layer_idx];

        let total_layer_height = layer_heights.get(layer_idx).copied().unwrap_or(node_height);
        let start_y = (canvas_height.saturating_sub(total_layer_height)) / 2;
        let mut y = start_y;

        for (node_idx, id) in layer.iter().enumerate() {
            let w = widths.get(id).copied().unwrap_or(7);
            let h = node_heights.get(id).copied().unwrap_or(node_height);
            let cx = col_x + col_w / 2;
            layer_by_id.insert(id.clone(), layer_idx);

            if let Some(node) = graph.nodes.get(id) {
                canvas.draw_node_with_height(
                    cx,
                    y,
                    w,
                    h,
                    &node.label,
                    node.shape,
                    border_fg,
                    text_fg,
                );
            }

            positions.insert(
                id.clone(),
                NodeLayout {
                    center_x: cx,
                    top_y: y,
                    width: w,
                    height: h,
                },
            );

            if node_idx + 1 < layer.len() {
                let next_id = &layer[node_idx + 1];
                y +=
                    h + v_gap + lr_node_extra_gap(graph, id).max(lr_node_extra_gap(graph, next_id));
            }
        }

        col_x += col_w + node_h_gap;
    }

    // Draw edges
    let (outgoing_ports, incoming_ports) = lr_edge_port_maps(graph, &positions);
    let lane_counts = lr_lane_counts(
        graph,
        &positions,
        &layer_by_id,
        &outgoing_ports,
        &incoming_ports,
    );
    let mut lane_seen: HashMap<(usize, usize), usize> = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if let (Some(src), Some(dst)) = (positions.get(&edge.from), positions.get(&edge.to)) {
            let src_right_x = node_right_x(src);
            let dst_left_x = node_left_x(dst);
            let (src_port_idx, src_port_count) = outgoing_ports[edge_idx];
            let (dst_port_idx, dst_port_count) = incoming_ports[edge_idx];
            let src_cy = lr_edge_port_y(src.top_y, src.height, src_port_idx, src_port_count);
            let dst_cy = lr_edge_port_y(dst.top_y, dst.height, dst_port_idx, dst_port_count);
            let edge_fg = Some(edge_color(theme, edge_idx));
            let mid_x_override = if src_cy == dst_cy {
                None
            } else {
                lr_lane_key(edge, &layer_by_id).and_then(|key| {
                    let lane_idx = lane_seen.entry(key).or_insert(0);
                    let current_lane = *lane_idx;
                    *lane_idx += 1;
                    lr_lane_mid_x(
                        src_right_x,
                        dst_left_x,
                        current_lane,
                        lane_counts.get(&key).copied().unwrap_or(1),
                    )
                })
            };

            canvas.draw_edge_lr(
                src.center_x,
                src_right_x,
                src_cy,
                dst_left_x,
                dst_cy,
                EdgeStyle {
                    head: EdgeEnd::Arrow,
                    label: edge.label.as_deref(),
                    ..Default::default()
                },
                edge_fg,
                mid_x_override,
            );
        }
    }

    let rows = canvas.to_span_rows(theme);
    Some((rows, canvas_width))
}

// ───── Public entry for the dispatcher ─────

pub(crate) fn render(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let graph = parse_mermaid(code)?;
    match graph.direction {
        Direction::TopDown => render_td(&graph, theme),
        Direction::LeftRight => render_lr(&graph, theme),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::render_mermaid;
    use super::super::adjacent_layer_crossing_score;
    use super::*;
    use crate::style::StyledSpan;
    use crate::theme::Theme;
    use std::collections::HashSet;

    fn row_text(row: &[StyledSpan]) -> String {
        row.iter().map(|span| span.text.as_str()).collect()
    }

    fn char_col(text: &str, byte_idx: usize) -> usize {
        text[..byte_idx].chars().count()
    }

    fn lr_routing_gap_cells(input: &str) -> Vec<(usize, char)> {
        let theme = Theme::dark();
        let (rows, _) = render_mermaid(input, &theme).expect("expected rendered diagram");
        let texts: Vec<String> = rows.iter().map(|row| row_text(row)).collect();

        let source_right = texts
            .iter()
            .find_map(|row| {
                row.find("│ Source │")
                    .map(|x| char_col(row, x) + "│ Source │".chars().count())
            })
            .expect("expected source node");
        let destination_left = texts
            .iter()
            .filter_map(|row| {
                ["│ One │", "│ Two │", "│ Three │", "│ Four │"]
                    .iter()
                    .filter_map(|label| row.find(label).map(|x| char_col(row, x)))
                    .min()
            })
            .min()
            .expect("expected destination nodes");

        texts
            .iter()
            .flat_map(|row| {
                row.chars()
                    .enumerate()
                    .filter_map(|(x, ch)| {
                        (x > source_right && x < destination_left && ch != ' ').then_some((x, ch))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn lr_edges_use_separate_routing_lanes() {
        let routing_columns: HashSet<usize> = lr_routing_gap_cells(
            "graph LR\nA[Source] --> B[One]\nA --> C[Two]\nA --> D[Three]\nA --> E[Four]",
        )
        .into_iter()
        .filter_map(|(x, ch)| {
            matches!(
                ch,
                '│' | '╭' | '╮' | '╰' | '╯' | '┌' | '┐' | '└' | '┘' | '┬' | '┴' | '┼'
            )
            .then_some(x)
        })
        .collect();

        assert!(
            routing_columns.len() >= 3,
            "multiple LR edges should spread across multiple routing lanes"
        );
    }

    #[test]
    fn lr_routing_gap_avoids_merge_junctions() {
        let has_merge = lr_routing_gap_cells(
            "graph LR\nA[Source] --> B[One]\nA --> C[Two]\nA --> D[Three]\nA --> E[Four]",
        )
        .into_iter()
        .any(|(_, ch)| matches!(ch, '├' | '┤' | '┬' | '┴' | '┼'));

        assert!(
            !has_merge,
            "LR per-edge rounded paths should avoid merge/cross junction glyphs"
        );
    }

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

        assert_eq!(
            adjacent_layer_crossing_score(&crossing_layers, &graph, 0),
            1
        );
        assert_eq!(adjacent_layer_crossing_score(&aligned_layers, &graph, 0), 0);
    }
}
