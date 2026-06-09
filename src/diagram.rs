use crate::style::{Style, StyledSpan};
use crate::theme::Theme;
use crossterm::style::Color;
use std::collections::{HashMap, HashSet, VecDeque};

// ───── Data types ─────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Direction {
    TopDown,
    LeftRight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NodeShape {
    Rectangle,
    Rounded,
    Diamond,
    Circle,
}

/// A row to be drawn inside a multi-row card node.
pub(crate) struct CardDrawRow {
    pub key: String,
    pub value_text: String,
    pub value_color: Option<Color>,
    /// If true, the value area shows `──▶` instead of text.
    pub is_connector: bool,
}

#[derive(Debug, Clone)]
struct Node {
    label: String,
    shape: NodeShape,
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
}

#[derive(Debug)]
struct Graph {
    direction: Direction,
    nodes: HashMap<String, Node>,
    edges: Vec<Edge>,
    node_order: Vec<String>,
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
fn parse_node_ref(s: &str) -> Option<(String, Option<(String, NodeShape)>, &str)> {
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

fn parse_arrow(s: &str) -> Option<(Option<String>, &str)> {
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

// ───── Layout ─────

#[derive(Clone)]
pub(crate) struct NodeLayout {
    pub(crate) center_x: usize,
    pub(crate) top_y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

fn assign_layers(graph: &Graph) -> Vec<Vec<String>> {
    // Build adjacency and in-degree
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for id in graph.nodes.keys() {
        in_degree.entry(id.as_str()).or_insert(0);
        adj.entry(id.as_str()).or_default();
    }

    for edge in &graph.edges {
        adj.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
        *in_degree.entry(edge.to.as_str()).or_insert(0) += 1;
    }

    // Kahn's topological sort
    let mut queue: VecDeque<&str> = VecDeque::new();
    let mut topo_order: Vec<String> = Vec::new();
    let mut in_deg = in_degree.clone();

    for id in &graph.node_order {
        if in_deg.get(id.as_str()).copied().unwrap_or(0) == 0 {
            queue.push_back(id.as_str());
        }
    }

    // Cycle fallback
    if queue.is_empty()
        && let Some(first) = graph.node_order.first()
    {
        queue.push_back(first.as_str());
    }

    while let Some(node) = queue.pop_front() {
        topo_order.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                let deg = in_deg.get_mut(next).unwrap();
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }
    }

    // Add any remaining nodes (from cycles)
    for id in &graph.node_order {
        if !topo_order.contains(id) {
            topo_order.push(id.clone());
        }
    }

    // Longest-path layer assignment
    let mut node_layer: HashMap<String, usize> = HashMap::new();
    for node in &topo_order {
        let mut max_parent_layer: Option<usize> = None;
        for edge in &graph.edges {
            if edge.to == *node
                && let Some(&parent_layer) = node_layer.get(&edge.from)
            {
                max_parent_layer =
                    Some(max_parent_layer.map_or(parent_layer, |m: usize| m.max(parent_layer)));
            }
        }
        let layer = max_parent_layer.map_or(0, |m| m + 1);
        node_layer.insert(node.clone(), layer);
    }

    let max_layer = node_layer.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_layer + 1];
    for node in &topo_order {
        let layer = node_layer[node];
        layers[layer].push(node.clone());
    }
    layers.retain(|l| !l.is_empty());
    layers
}

fn order_within_layers(layers: &mut [Vec<String>], graph: &Graph) {
    // Barycenter heuristic to reduce edge crossings
    for _ in 0..4 {
        // Forward pass
        for i in 1..layers.len() {
            let prev_layer = layers[i - 1].clone();
            let mut positions: Vec<(String, f64)> = Vec::new();

            for node in &layers[i] {
                let mut parent_positions: Vec<f64> = Vec::new();
                for edge in &graph.edges {
                    if edge.to == *node
                        && let Some(pos) = prev_layer.iter().position(|n| n == &edge.from)
                    {
                        parent_positions.push(pos as f64);
                    }
                }
                let avg = if parent_positions.is_empty() {
                    layers[i].iter().position(|n| n == node).unwrap_or(0) as f64
                } else {
                    parent_positions.iter().sum::<f64>() / parent_positions.len() as f64
                };
                positions.push((node.clone(), avg));
            }
            positions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            layers[i] = positions.into_iter().map(|(n, _)| n).collect();
        }

        // Backward pass
        for i in (0..layers.len().saturating_sub(1)).rev() {
            let next_layer = layers[i + 1].clone();
            let mut positions: Vec<(String, f64)> = Vec::new();

            for node in &layers[i] {
                let mut child_positions: Vec<f64> = Vec::new();
                for edge in &graph.edges {
                    if edge.from == *node
                        && let Some(pos) = next_layer.iter().position(|n| n == &edge.to)
                    {
                        child_positions.push(pos as f64);
                    }
                }
                let avg = if child_positions.is_empty() {
                    layers[i].iter().position(|n| n == node).unwrap_or(0) as f64
                } else {
                    child_positions.iter().sum::<f64>() / child_positions.len() as f64
                };
                positions.push((node.clone(), avg));
            }
            positions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            layers[i] = positions.into_iter().map(|(n, _)| n).collect();
        }
    }
}

fn adjacent_layer_crossing_score(
    layers: &[Vec<String>],
    graph: &Graph,
    left_layer: usize,
) -> usize {
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

fn lr_node_extra_gap(graph: &Graph, node_id: &str) -> usize {
    let degree = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node_id || edge.to == node_id)
        .count();

    match degree {
        0 | 1 => 0,
        2 | 3 => 2,
        4 | 5 => 3,
        _ => 4,
    }
}

fn lr_node_port_count(graph: &Graph, node_id: &str) -> usize {
    let outgoing = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node_id)
        .count();
    let incoming = graph.edges.iter().filter(|edge| edge.to == node_id).count();
    outgoing.max(incoming).max(1)
}

fn lr_node_height(graph: &Graph, node_id: &str) -> usize {
    let port_count = lr_node_port_count(graph, node_id);
    if port_count <= 1 {
        3
    } else {
        port_count * 3 + 1
    }
}

fn lr_layer_height(
    layer: &[String],
    graph: &Graph,
    node_heights: &HashMap<String, usize>,
    base_gap: usize,
) -> usize {
    if layer.is_empty() {
        return 0;
    }

    let nodes_height = layer
        .iter()
        .map(|id| {
            node_heights
                .get(id)
                .copied()
                .unwrap_or_else(|| lr_node_height(graph, id))
        })
        .sum::<usize>();
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

fn node_box_width(node: &Node) -> usize {
    label_box_width(&node.label, node.shape)
}

pub(crate) fn label_box_width(label: &str, shape: NodeShape) -> usize {
    let label_width = label.chars().count();
    let width = match shape {
        NodeShape::Diamond => label_width + 6,
        _ => label_width + 4,
    };
    width.max(7)
}

// ───── Canvas ─────

pub(crate) const CONN_UP: u8 = 1;
pub(crate) const CONN_DOWN: u8 = 2;
pub(crate) const CONN_LEFT: u8 = 4;
pub(crate) const CONN_RIGHT: u8 = 8;

pub(crate) fn junction_char(connects: u8) -> char {
    match connects {
        c if c == CONN_UP | CONN_DOWN => '│',
        c if c == CONN_LEFT | CONN_RIGHT => '─',
        c if c == CONN_DOWN | CONN_RIGHT => '╭',
        c if c == CONN_DOWN | CONN_LEFT => '╮',
        c if c == CONN_UP | CONN_RIGHT => '╰',
        c if c == CONN_UP | CONN_LEFT => '╯',
        c if c == CONN_UP | CONN_DOWN | CONN_RIGHT => '├',
        c if c == CONN_UP | CONN_DOWN | CONN_LEFT => '┤',
        c if c == CONN_DOWN | CONN_LEFT | CONN_RIGHT => '┬',
        c if c == CONN_UP | CONN_LEFT | CONN_RIGHT => '┴',
        c if c == CONN_UP | CONN_DOWN | CONN_LEFT | CONN_RIGHT => '┼',
        c if c == CONN_UP => '│',
        c if c == CONN_DOWN => '│',
        c if c == CONN_LEFT => '─',
        c if c == CONN_RIGHT => '─',
        _ => '·',
    }
}

fn edge_char_connects(ch: char) -> Option<u8> {
    match ch {
        '│' => Some(CONN_UP | CONN_DOWN),
        '─' => Some(CONN_LEFT | CONN_RIGHT),
        '╭' => Some(CONN_DOWN | CONN_RIGHT),
        '╮' => Some(CONN_DOWN | CONN_LEFT),
        '╰' => Some(CONN_UP | CONN_RIGHT),
        '╯' => Some(CONN_UP | CONN_LEFT),
        '├' => Some(CONN_UP | CONN_DOWN | CONN_RIGHT),
        '┤' => Some(CONN_UP | CONN_DOWN | CONN_LEFT),
        '┬' => Some(CONN_DOWN | CONN_LEFT | CONN_RIGHT),
        '┴' => Some(CONN_UP | CONN_LEFT | CONN_RIGHT),
        '┼' => Some(CONN_UP | CONN_DOWN | CONN_LEFT | CONN_RIGHT),
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct CanvasCell {
    pub(crate) ch: char,
    pub(crate) fg: Option<Color>,
    pub(crate) bg: Option<Color>,
    pub(crate) is_node: bool,
    pub(crate) connects: u8,
}

impl Default for CanvasCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            is_node: false,
            connects: 0,
        }
    }
}

pub(crate) struct Canvas {
    pub(crate) width: usize,
    pub(crate) height: usize,
    cells: Vec<Vec<CanvasCell>>,
}

impl Canvas {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![vec![CanvasCell::default(); width]; height],
        }
    }

    pub(crate) fn set(&mut self, x: usize, y: usize, ch: char, fg: Option<Color>) {
        if y < self.height && x < self.width {
            self.cells[y][x].ch = ch;
            self.cells[y][x].fg = fg;
        }
    }

    pub(crate) fn set_node(&mut self, x: usize, y: usize, ch: char, fg: Option<Color>) {
        if y < self.height && x < self.width {
            self.cells[y][x].ch = ch;
            self.cells[y][x].fg = fg;
            self.cells[y][x].is_node = true;
        }
    }

    pub(crate) fn set_edge(&mut self, x: usize, y: usize, ch: char, fg: Option<Color>) {
        if y < self.height && x < self.width && !self.cells[y][x].is_node {
            let cell = &mut self.cells[y][x];
            if let (Some(existing), Some(incoming)) =
                (edge_char_connects(cell.ch), edge_char_connects(ch))
            {
                cell.connects = existing | incoming;
                cell.ch = junction_char(cell.connects);
            } else {
                cell.ch = ch;
                cell.connects = edge_char_connects(ch).unwrap_or(0);
            }
            if fg.is_some() {
                cell.fg = fg;
            }
        }
    }

    pub(crate) fn add_connection(&mut self, x: usize, y: usize, dir: u8, fg: Option<Color>) {
        if y < self.height && x < self.width {
            let cell = &mut self.cells[y][x];
            if !cell.is_node {
                cell.connects |= dir;
                cell.ch = junction_char(cell.connects);
                if fg.is_some() {
                    cell.fg = fg;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_node(
        &mut self,
        cx: usize,
        y: usize,
        width: usize,
        label: &str,
        shape: NodeShape,
        border_fg: Option<Color>,
        text_fg: Option<Color>,
    ) {
        let x = cx.saturating_sub(width / 2);

        let (tl, tr, bl, br, h, v) = match shape {
            NodeShape::Rectangle => ('┌', '┐', '└', '┘', '─', '│'),
            NodeShape::Rounded | NodeShape::Circle => ('╭', '╮', '╰', '╯', '─', '│'),
            NodeShape::Diamond => ('◆', '◆', '◆', '◆', '─', '│'),
        };

        // Top border
        self.set_node(x, y, tl, border_fg);
        for i in 1..width - 1 {
            self.set_node(x + i, y, h, border_fg);
        }
        self.set_node(x + width - 1, y, tr, border_fg);

        // Content line
        self.set_node(x, y + 1, v, border_fg);
        for i in 1..width - 1 {
            self.set_node(x + i, y + 1, ' ', text_fg);
        }
        let label_chars: Vec<char> = label.chars().collect();
        let padding = (width - 2).saturating_sub(label_chars.len());
        let left_pad = padding / 2;
        for (i, &ch) in label_chars.iter().enumerate() {
            if x + 1 + left_pad + i < x + width - 1 {
                self.set_node(x + 1 + left_pad + i, y + 1, ch, text_fg);
            }
        }
        self.set_node(x + width - 1, y + 1, v, border_fg);

        // Bottom border
        self.set_node(x, y + 2, bl, border_fg);
        for i in 1..width - 1 {
            self.set_node(x + i, y + 2, h, border_fg);
        }
        self.set_node(x + width - 1, y + 2, br, border_fg);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_node_with_height(
        &mut self,
        cx: usize,
        y: usize,
        width: usize,
        height: usize,
        label: &str,
        shape: NodeShape,
        border_fg: Option<Color>,
        text_fg: Option<Color>,
    ) {
        let height = height.max(3);
        let x = cx.saturating_sub(width / 2);

        let (tl, tr, bl, br, h, v) = match shape {
            NodeShape::Rectangle => ('┌', '┐', '└', '┘', '─', '│'),
            NodeShape::Rounded | NodeShape::Circle => ('╭', '╮', '╰', '╯', '─', '│'),
            NodeShape::Diamond => ('◆', '◆', '◆', '◆', '─', '│'),
        };

        self.set_node(x, y, tl, border_fg);
        for i in 1..width - 1 {
            self.set_node(x + i, y, h, border_fg);
        }
        self.set_node(x + width - 1, y, tr, border_fg);

        let label_y = y + height / 2;
        let label_chars: Vec<char> = label.chars().collect();
        let padding = (width - 2).saturating_sub(label_chars.len());
        let left_pad = padding / 2;

        for row_y in (y + 1)..(y + height - 1) {
            self.set_node(x, row_y, v, border_fg);
            for i in 1..width - 1 {
                self.set_node(x + i, row_y, ' ', text_fg);
            }
            if row_y == label_y {
                for (i, &ch) in label_chars.iter().enumerate() {
                    if x + 1 + left_pad + i < x + width - 1 {
                        self.set_node(x + 1 + left_pad + i, row_y, ch, text_fg);
                    }
                }
            }
            self.set_node(x + width - 1, row_y, v, border_fg);
        }

        let bottom_y = y + height - 1;
        self.set_node(x, bottom_y, bl, border_fg);
        for i in 1..width - 1 {
            self.set_node(x + i, bottom_y, h, border_fg);
        }
        self.set_node(x + width - 1, bottom_y, br, border_fg);
    }

    fn set_node_bg(&mut self, x: usize, y: usize, ch: char, fg: Option<Color>, bg: Option<Color>) {
        if y < self.height && x < self.width {
            self.cells[y][x].ch = ch;
            self.cells[y][x].fg = fg;
            self.cells[y][x].bg = bg;
            self.cells[y][x].is_node = true;
        }
    }

    /// Draw a multi-row card (table-like node) used by the JSON graph view.
    /// Returns the y-coordinate of each content row for edge routing.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_card(
        &mut self,
        left_x: usize,
        top_y: usize,
        width: usize,
        title: &str,
        rows: &[CardDrawRow],
        border_fg: Option<Color>,
        title_fg: Option<Color>,
        key_fg: Option<Color>,
        highlight_rows: &HashSet<usize>,
        highlight_fg: Option<Color>,
        card_highlight_bg: Option<Color>,
    ) -> Vec<usize> {
        if width < 4 {
            return Vec::new();
        }
        let inner = width - 2; // space between │ and │
        let bg = card_highlight_bg;

        // ── top border with title: ╭─ title ─────╮ ──
        self.set_node_bg(left_x, top_y, '╭', border_fg, bg);
        self.set_node_bg(left_x + 1, top_y, '─', border_fg, bg);
        let title_chars: Vec<char> = title.chars().collect();
        let max_title = inner.saturating_sub(3); // "─" + space on each side
        let show_title = title_chars.len().min(max_title);
        self.set_node_bg(left_x + 2, top_y, ' ', title_fg, bg);
        for (i, &ch) in title_chars[..show_title].iter().enumerate() {
            self.set_node_bg(left_x + 3 + i, top_y, ch, title_fg, bg);
        }
        let fill_start = left_x + 3 + show_title;
        self.set_node_bg(fill_start, top_y, ' ', border_fg, bg);
        for x in (fill_start + 1)..(left_x + width - 1) {
            self.set_node_bg(x, top_y, '─', border_fg, bg);
        }
        self.set_node_bg(left_x + width - 1, top_y, '╮', border_fg, bg);

        // ── content rows ──
        let key_col_width = rows
            .iter()
            .map(|r| r.key.chars().count())
            .max()
            .unwrap_or(0)
            .min(inner.saturating_sub(4));

        let mut row_ys = Vec::with_capacity(rows.len());
        for (ri, row) in rows.iter().enumerate() {
            let y = top_y + 1 + ri;
            row_ys.push(y);

            let is_highlight = highlight_rows.contains(&ri);
            let row_key_fg = if is_highlight { highlight_fg } else { key_fg };
            let row_val_fg = if is_highlight {
                highlight_fg
            } else {
                row.value_color
            };

            // left border
            self.set_node_bg(left_x, y, '│', border_fg, bg);

            // space after border
            self.set_node_bg(left_x + 1, y, ' ', row_key_fg, bg);

            // key text
            let key_chars: Vec<char> = row.key.chars().collect();
            let show_key = key_chars.len().min(key_col_width);
            for (i, &ch) in key_chars[..show_key].iter().enumerate() {
                self.set_node_bg(left_x + 2 + i, y, ch, row_key_fg, bg);
            }
            // pad key column
            for i in show_key..key_col_width {
                self.set_node_bg(left_x + 2 + i, y, ' ', row_key_fg, bg);
            }

            // gap between key and value
            let val_start = left_x + 2 + key_col_width + 1;
            self.set_node_bg(val_start - 1, y, ' ', row_val_fg, bg);

            // value text (fill remaining space)
            let val_space = (left_x + width - 1).saturating_sub(val_start + 1);
            if row.is_connector {
                // draw ──▶ at the right edge of the card
                for x in val_start..(left_x + width - 1) {
                    self.set_node_bg(x, y, ' ', row_val_fg, bg);
                }
                // put the arrow near the right border
                let arrow_start = (left_x + width - 1).saturating_sub(4);
                if arrow_start >= val_start {
                    self.set_node_bg(arrow_start, y, '─', row_val_fg, bg);
                    self.set_node_bg(arrow_start + 1, y, '─', row_val_fg, bg);
                    self.set_node_bg(arrow_start + 2, y, '▶', row_val_fg, bg);
                }
            } else {
                let val_chars: Vec<char> = row.value_text.chars().collect();
                let show_val = val_chars.len().min(val_space);
                for (i, &ch) in val_chars[..show_val].iter().enumerate() {
                    self.set_node_bg(val_start + i, y, ch, row_val_fg, bg);
                }
                // pad remaining
                for i in show_val..val_space {
                    self.set_node_bg(val_start + i, y, ' ', row_val_fg, bg);
                }
            }

            // space before right border
            self.set_node_bg(left_x + width - 2, y, ' ', border_fg, bg);
            // right border
            self.set_node_bg(left_x + width - 1, y, '│', border_fg, bg);
        }

        // ── bottom border: ╰─────────╯ ──
        let bot_y = top_y + 1 + rows.len();
        self.set_node_bg(left_x, bot_y, '╰', border_fg, bg);
        for x in (left_x + 1)..(left_x + width - 1) {
            self.set_node_bg(x, bot_y, '─', border_fg, bg);
        }
        self.set_node_bg(left_x + width - 1, bot_y, '╯', border_fg, bg);

        row_ys
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_edge_td(
        &mut self,
        src_cx: usize,
        src_bottom_y: usize,
        dst_cx: usize,
        dst_top_y: usize,
        label: Option<&str>,
        edge_fg: Option<Color>,
        label_fg: Option<Color>,
    ) {
        if src_bottom_y + 1 >= dst_top_y {
            return;
        }

        let mid_y = src_bottom_y + 1 + (dst_top_y - src_bottom_y - 1) / 2;

        if src_cx == dst_cx {
            // Straight down
            for y in (src_bottom_y + 1)..dst_top_y {
                self.add_connection(src_cx, y, CONN_UP | CONN_DOWN, edge_fg);
            }
            // Arrow replaces last segment
            self.set(dst_cx, dst_top_y - 1, '▼', edge_fg);

            // Place label beside the vertical line
            if let Some(text) = label {
                let label_y = src_bottom_y + 1;
                for (i, ch) in text.chars().enumerate() {
                    self.set(src_cx + 2 + i, label_y, ch, label_fg);
                }
            }
        } else {
            // Down from source to mid_y
            for y in (src_bottom_y + 1)..mid_y {
                self.add_connection(src_cx, y, CONN_UP | CONN_DOWN, edge_fg);
            }

            // Junction at source column, mid_y
            let src_turn = if dst_cx > src_cx {
                CONN_UP | CONN_RIGHT
            } else {
                CONN_UP | CONN_LEFT
            };
            self.add_connection(src_cx, mid_y, src_turn, edge_fg);

            // Horizontal segment
            let (min_x, max_x) = if src_cx < dst_cx {
                (src_cx, dst_cx)
            } else {
                (dst_cx, src_cx)
            };
            for x in (min_x + 1)..max_x {
                self.add_connection(x, mid_y, CONN_LEFT | CONN_RIGHT, edge_fg);
            }

            // Junction at destination column, mid_y
            let dst_turn = if dst_cx > src_cx {
                CONN_LEFT | CONN_DOWN
            } else {
                CONN_RIGHT | CONN_DOWN
            };
            self.add_connection(dst_cx, mid_y, dst_turn, edge_fg);

            // Down from mid_y to destination
            for y in (mid_y + 1)..dst_top_y {
                self.add_connection(dst_cx, y, CONN_UP | CONN_DOWN, edge_fg);
            }

            // Arrow
            self.set(dst_cx, dst_top_y - 1, '▼', edge_fg);

            // Place label above horizontal segment
            if let Some(text) = label {
                let label_len = text.chars().count();
                let label_start = min_x + (max_x - min_x).saturating_sub(label_len) / 2;
                let label_y = if mid_y > 0 { mid_y - 1 } else { mid_y };
                for (i, ch) in text.chars().enumerate() {
                    let lx = label_start + i;
                    if lx < self.width {
                        self.set(lx, label_y, ch, label_fg);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_edge_lr(
        &mut self,
        _src_cx: usize,
        src_right_x: usize,
        src_cy: usize,
        dst_left_x: usize,
        dst_cy: usize,
        label: Option<&str>,
        edge_fg: Option<Color>,
        label_fg: Option<Color>,
        mid_x_override: Option<usize>,
    ) {
        if src_right_x + 1 >= dst_left_x {
            return;
        }

        let mid_x =
            mid_x_override.unwrap_or_else(|| src_right_x + 1 + (dst_left_x - src_right_x - 1) / 2);

        if src_cy == dst_cy {
            // Straight right
            for x in (src_right_x + 1)..dst_left_x {
                self.set_edge(x, src_cy, '─', edge_fg);
            }
            // Arrow replaces last segment
            self.set_edge(dst_left_x - 1, dst_cy, '▶', edge_fg);

            // Label above the horizontal line
            if let Some(text) = label {
                let label_x = src_right_x + 2;
                let label_y = if src_cy > 0 { src_cy - 1 } else { 0 };
                for (i, ch) in text.chars().enumerate() {
                    self.set(label_x + i, label_y, ch, label_fg);
                }
            }
        } else {
            // Right from source to mid_x
            for x in (src_right_x + 1)..mid_x {
                self.set_edge(x, src_cy, '─', edge_fg);
            }

            // Rounded turn at source lane
            let src_turn = if dst_cy > src_cy { '╮' } else { '╯' };
            self.set_edge(mid_x, src_cy, src_turn, edge_fg);

            // Vertical segment
            if src_cy < dst_cy {
                for y in (src_cy + 1)..dst_cy {
                    self.set_edge(mid_x, y, '│', edge_fg);
                }
            } else {
                for y in (dst_cy + 1)..src_cy {
                    self.set_edge(mid_x, y, '│', edge_fg);
                }
            }

            // Rounded turn toward destination
            let dst_turn = if dst_cy > src_cy { '╰' } else { '╭' };
            self.set_edge(mid_x, dst_cy, dst_turn, edge_fg);

            // Right from mid_x to destination
            for x in (mid_x + 1)..dst_left_x {
                self.set_edge(x, dst_cy, '─', edge_fg);
            }

            // Arrow
            self.set_edge(dst_left_x - 1, dst_cy, '▶', edge_fg);

            // Label near the vertical segment
            if let Some(text) = label {
                let (min_y, max_y) = if src_cy < dst_cy {
                    (src_cy, dst_cy)
                } else {
                    (dst_cy, src_cy)
                };
                let label_y = min_y + (max_y - min_y).saturating_sub(1) / 2;
                for (i, ch) in text.chars().enumerate() {
                    self.set(mid_x + 2 + i, label_y, ch, label_fg);
                }
            }
        }
    }

    pub(crate) fn to_span_rows(&self, theme: &Theme) -> Vec<Vec<StyledSpan>> {
        let default_bg = Some(theme.code_bg);
        self.cells
            .iter()
            .map(|row| {
                let mut spans = Vec::new();
                let mut i = 0;
                while i < row.len() {
                    let fg = row[i].fg.unwrap_or(theme.fg);
                    let cell_bg = row[i].bg.or(default_bg);
                    let mut text = String::new();
                    let mut j = i;
                    while j < row.len()
                        && row[j].fg.unwrap_or(theme.fg) == fg
                        && row[j].bg.or(default_bg) == cell_bg
                    {
                        text.push(row[j].ch);
                        j += 1;
                    }
                    spans.push(StyledSpan {
                        text,
                        style: Style {
                            fg: Some(fg),
                            bg: cell_bg,
                            ..Default::default()
                        },
                    });
                    i = j;
                }
                spans
            })
            .collect()
    }
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
                edge.label.as_deref(),
                edge_fg,
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
                edge.label.as_deref(),
                edge_fg,
                edge_fg,
                mid_x_override,
            );
        }
    }

    let rows = canvas.to_span_rows(theme);
    Some((rows, canvas_width))
}

// ───── Public API ─────

fn lr_lane_key(edge: &Edge, layer_by_id: &HashMap<String, usize>) -> Option<(usize, usize)> {
    Some((*layer_by_id.get(&edge.from)?, *layer_by_id.get(&edge.to)?))
}

fn node_left_x(layout: &NodeLayout) -> usize {
    layout.center_x.saturating_sub(layout.width / 2)
}

fn node_right_x(layout: &NodeLayout) -> usize {
    node_left_x(layout) + layout.width.saturating_sub(1)
}

fn lr_lane_counts(
    graph: &Graph,
    positions: &HashMap<String, NodeLayout>,
    layer_by_id: &HashMap<String, usize>,
    outgoing_ports: &[(usize, usize)],
    incoming_ports: &[(usize, usize)],
) -> HashMap<(usize, usize), usize> {
    let mut counts = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if let (Some(src), Some(dst), Some(key)) = (
            positions.get(&edge.from),
            positions.get(&edge.to),
            lr_lane_key(edge, layer_by_id),
        ) {
            let (src_port_idx, src_port_count) = outgoing_ports[edge_idx];
            let (dst_port_idx, dst_port_count) = incoming_ports[edge_idx];
            let src_cy = lr_edge_port_y(src.top_y, src.height, src_port_idx, src_port_count);
            let dst_cy = lr_edge_port_y(dst.top_y, dst.height, dst_port_idx, dst_port_count);
            if src_cy != dst_cy {
                *counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn lr_lane_mid_x(
    src_right_x: usize,
    dst_left_x: usize,
    lane_idx: usize,
    lane_count: usize,
) -> Option<usize> {
    if lane_count <= 1 || src_right_x + 1 >= dst_left_x {
        return None;
    }

    let first = src_right_x + 1;
    let slots = dst_left_x.saturating_sub(first);
    if slots == 0 {
        return None;
    }

    let lane = lane_idx.min(lane_count - 1);
    Some(first + ((lane + 1) * slots / (lane_count + 1)))
}

fn lr_edge_port_y(top_y: usize, height: usize, port_idx: usize, port_count: usize) -> usize {
    if port_count <= 1 {
        return top_y + height / 2;
    }

    let content_rows = height.saturating_sub(2).max(1);
    if content_rows <= 1 {
        return top_y + 1;
    }

    let slot = port_idx.min(port_count - 1) * (content_rows - 1) / (port_count - 1);
    top_y + 1 + slot
}

fn lr_edge_port_maps(
    graph: &Graph,
    positions: &HashMap<String, NodeLayout>,
) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let mut outgoing_ports = vec![(0, 1); graph.edges.len()];
    let mut incoming_ports = vec![(0, 1); graph.edges.len()];

    let mut outgoing_by_node: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut incoming_by_node: HashMap<&str, Vec<usize>> = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        outgoing_by_node
            .entry(edge.from.as_str())
            .or_default()
            .push(edge_idx);
        incoming_by_node
            .entry(edge.to.as_str())
            .or_default()
            .push(edge_idx);
    }

    for edge_indices in outgoing_by_node.values_mut() {
        edge_indices.sort_by_key(|&edge_idx| {
            let edge = &graph.edges[edge_idx];
            let dst_y = positions
                .get(&edge.to)
                .map(|layout| layout.top_y + layout.height / 2)
                .unwrap_or(usize::MAX);
            (dst_y, edge_idx)
        });
        let port_count = edge_indices.len();
        for (port_idx, &edge_idx) in edge_indices.iter().enumerate() {
            outgoing_ports[edge_idx] = (port_idx, port_count);
        }
    }

    for edge_indices in incoming_by_node.values_mut() {
        edge_indices.sort_by_key(|&edge_idx| {
            let edge = &graph.edges[edge_idx];
            let src_y = positions
                .get(&edge.from)
                .map(|layout| layout.top_y + layout.height / 2)
                .unwrap_or(usize::MAX);
            (src_y, edge_idx)
        });
        let port_count = edge_indices.len();
        for (port_idx, &edge_idx) in edge_indices.iter().enumerate() {
            incoming_ports[edge_idx] = (port_idx, port_count);
        }
    }

    (outgoing_ports, incoming_ports)
}

fn edge_color(theme: &Theme, index: usize) -> Color {
    let colors = if theme.name() == "dark" {
        [
            Color::Rgb {
                r: 0,
                g: 215,
                b: 255,
            },
            Color::Rgb {
                r: 255,
                g: 176,
                b: 0,
            },
            Color::Rgb {
                r: 255,
                g: 95,
                b: 215,
            },
            Color::Rgb {
                r: 95,
                g: 255,
                b: 135,
            },
            Color::Rgb {
                r: 255,
                g: 95,
                b: 95,
            },
            Color::Rgb {
                r: 175,
                g: 135,
                b: 255,
            },
            Color::Rgb {
                r: 255,
                g: 255,
                b: 95,
            },
            Color::Rgb {
                r: 95,
                g: 175,
                b: 255,
            },
            Color::Rgb {
                r: 0,
                g: 255,
                b: 215,
            },
            Color::Rgb {
                r: 255,
                g: 135,
                b: 95,
            },
        ]
    } else {
        [
            Color::Rgb {
                r: 0,
                g: 92,
                b: 197,
            },
            Color::Rgb {
                r: 211,
                g: 86,
                b: 0,
            },
            Color::Rgb {
                r: 159,
                g: 0,
                b: 136,
            },
            Color::Rgb {
                r: 0,
                g: 115,
                b: 73,
            },
            Color::Rgb {
                r: 203,
                g: 36,
                b: 49,
            },
            Color::Rgb {
                r: 93,
                g: 63,
                b: 211,
            },
            Color::Rgb {
                r: 140,
                g: 104,
                b: 0,
            },
            Color::Rgb {
                r: 0,
                g: 118,
                b: 168,
            },
            Color::Rgb {
                r: 0,
                g: 128,
                b: 128,
            },
            Color::Rgb {
                r: 170,
                g: 70,
                b: 20,
            },
        ]
    };

    colors[index % colors.len()]
}

/// Try to render mermaid code as a visual diagram.
/// Returns (content_rows, content_width) or None if parsing fails.
pub fn render_mermaid(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let graph = parse_mermaid(code)?;
    match graph.direction {
        Direction::TopDown => render_td(&graph, theme),
        Direction::LeftRight => render_lr(&graph, theme),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn set_edge_marks_crossing_segments() {
        let mut canvas = Canvas::new(3, 3);

        canvas.set_edge(1, 1, '─', None);
        canvas.set_edge(1, 1, '│', None);

        assert_eq!(canvas.cells[1][1].ch, '┼');
    }

    #[test]
    fn simple_edge_turns_use_rounded_corners() {
        assert_eq!(junction_char(CONN_DOWN | CONN_RIGHT), '╭');
        assert_eq!(junction_char(CONN_DOWN | CONN_LEFT), '╮');
        assert_eq!(junction_char(CONN_UP | CONN_RIGHT), '╰');
        assert_eq!(junction_char(CONN_UP | CONN_LEFT), '╯');

        assert_eq!(
            junction_char(CONN_UP | CONN_DOWN | CONN_LEFT | CONN_RIGHT),
            '┼'
        );
        assert_eq!(junction_char(CONN_DOWN | CONN_LEFT | CONN_RIGHT), '┬');
    }
}
