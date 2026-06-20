use std::collections::{HashMap, HashSet};

use crossterm::style::Color;

use crate::style::StyledSpan;
use crate::theme::Theme;

use super::super::canvas::{Canvas, CardDrawRow, EdgeEnd, EdgeStyle};
use super::super::theme::edge_color;
use super::flowchart::{Edge as FlEdge, Graph as FlGraph, Node as FlNode};
use super::{assign_layers, order_within_layers};

// ───── Data types ─────

#[derive(Debug, Clone, Copy, PartialEq)]
enum LineKind {
    Solid,
    Dashed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RelationshipStyle {
    line: LineKind,
    head: EdgeEnd,
    tail: EdgeEnd,
}

#[derive(Debug, Clone)]
struct ClassNode {
    /// Base identifier (no generics). Used as the relationship lookup key.
    name: String,
    /// Display title — `name` plus generics when present (e.g. `Foo<T, K>`).
    title: String,
    stereotype: Option<String>,
    members: Vec<MemberRow>,
}

#[derive(Debug, Clone)]
struct MemberRow {
    visibility: char,
    text: String,
}

#[derive(Debug, Clone)]
struct ClassEdge {
    from: String,
    to: String,
    style: RelationshipStyle,
    cardinalities: (Option<String>, Option<String>),
    label: Option<String>,
}

// ───── Public entry ─────

pub(crate) fn render(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let (nodes, edges) = parse_class_diagram(code)?;
    render_class(&nodes, &edges, theme)
}

// ───── Parser ─────

fn parse_class_diagram(code: &str) -> Option<(Vec<ClassNode>, Vec<ClassEdge>)> {
    let mut nodes: HashMap<String, ClassNode> = HashMap::new();
    let mut node_order: Vec<String> = Vec::new();
    let mut edges: Vec<ClassEdge> = Vec::new();

    let mut lines = code.lines().peekable();
    let mut header_seen = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        if !header_seen {
            if trimmed == "classDiagram" || trimmed == "classDiagram-v2" {
                header_seen = true;
                continue;
            } else {
                return None;
            }
        }

        if trimmed.starts_with("namespace ")
            || trimmed.starts_with("direction ")
            || trimmed.starts_with("skinparam")
            || trimmed.starts_with("style ")
        {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("class ") {
            let (base_name, title, inline_stereo, has_body) = parse_class_header(rest)?;

            let raw_body = if has_body {
                let mut body: Vec<String> = Vec::new();
                for body_line in lines.by_ref() {
                    let bt = body_line.trim();
                    if bt == "}" {
                        break;
                    }
                    if bt.is_empty() {
                        continue;
                    }
                    body.push(bt.to_string());
                }
                body
            } else {
                Vec::new()
            };

            // Split raw body rows into a stereotype (if any) and real members.
            let mut stereotype = inline_stereo;
            let mut members: Vec<MemberRow> = Vec::new();
            for raw in parse_class_body(&raw_body) {
                if let Some(s) = parse_as_stereotype(&raw.text) {
                    if stereotype.is_none() {
                        stereotype = Some(s);
                    }
                } else {
                    members.push(raw);
                }
            }

            let node = ClassNode {
                name: base_name.clone(),
                title,
                stereotype,
                members,
            };

            if nodes.contains_key(&base_name) {
                nodes.insert(base_name.clone(), node);
            } else {
                nodes.insert(base_name.clone(), node);
                node_order.push(base_name);
            }
            continue;
        }

        if let Some(edge) = parse_relationship(trimmed) {
            for name in [&edge.from, &edge.to] {
                if !nodes.contains_key(name) {
                    nodes.insert(
                        name.clone(),
                        ClassNode {
                            name: name.clone(),
                            title: name.clone(),
                            stereotype: None,
                            members: Vec::new(),
                        },
                    );
                    node_order.push(name.clone());
                }
            }
            edges.push(edge);
            continue;
        }
    }

    if nodes.is_empty() {
        return None;
    }

    let nodes_vec = node_order
        .into_iter()
        .filter_map(|n| nodes.remove(&n))
        .collect();
    Some((nodes_vec, edges))
}

/// Parse the text after `class `: identifier, optional generics, optional
/// inline stereotype `[<<...>>]`, optional body marker `{`.
fn parse_class_header(rest: &str) -> Option<(String, String, Option<String>, bool)> {
    let rest = rest.trim();
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let base_name = rest[..name_end].to_string();
    let mut rest = rest[name_end..].trim_start();

    let mut title = base_name.clone();
    if rest.starts_with('<')
        && let Some(end) = find_matching_angle(rest)
    {
        let generics = &rest[..=end];
        title = format!("{base_name}{generics}");
        rest = rest[end + 1..].trim_start();
    }

    let mut stereotype = None;
    if rest.starts_with('[')
        && let Some(end) = rest.find(']')
    {
        let content = rest[1..end].trim();
        if content.starts_with("<<") && content.ends_with(">>") {
            stereotype = Some(content.to_string());
        }
        rest = rest[end + 1..].trim_start();
    }

    let has_body = rest == "{" || rest.starts_with('{');
    Some((base_name, title, stereotype, has_body))
}

fn find_matching_angle(s: &str) -> Option<usize> {
    let mut depth: usize = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_class_body(lines: &[String]) -> Vec<MemberRow> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let first = trimmed.chars().next()?;
            if matches!(first, '+' | '-' | '#' | '~') {
                let text = trimmed[1..].trim_start().to_string();
                Some(MemberRow {
                    visibility: first,
                    text,
                })
            } else {
                Some(MemberRow {
                    visibility: ' ',
                    text: trimmed.to_string(),
                })
            }
        })
        .collect()
}

fn parse_as_stereotype(text: &str) -> Option<String> {
    let t = text.trim();
    if t.starts_with("<<") && t.ends_with(">>") {
        Some(t.to_string())
    } else {
        None
    }
}

/// Parse `A [card] OP [card] B [: label]` into a `ClassEdge`.
fn parse_relationship(line: &str) -> Option<ClassEdge> {
    let s = line.trim();

    let first_end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(s.len());
    if first_end == 0 {
        return None;
    }
    let from = s[..first_end].to_string();
    let mut rest = s[first_end..].trim_start();

    let from_card = take_quoted(&mut rest);

    let op_end = rest
        .find(|c: char| !matches!(c, '<' | '>' | '|' | '*' | 'o' | '-' | '.' | 'x'))
        .unwrap_or(rest.len());
    if op_end == 0 {
        return None;
    }
    let op = rest[..op_end].to_string();
    rest = rest[op_end..].trim_start();

    let to_card = take_quoted(&mut rest);

    let second_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if second_end == 0 {
        return None;
    }
    let to = rest[..second_end].to_string();
    rest = rest[second_end..].trim_start();

    let label = if let Some(stripped) = rest.strip_prefix(':') {
        let l = stripped.trim().to_string();
        if l.is_empty() { None } else { Some(l) }
    } else {
        None
    };

    let style = decode_relationship(&op)?;

    Some(ClassEdge {
        from,
        to,
        style,
        cardinalities: (from_card, to_card),
        label,
    })
}

/// If `s` starts with `"…"`, strip the quoted text and return it; otherwise
/// return `None`. The slice is advanced past the closing quote plus trailing
/// whitespace.
fn take_quoted(s: &mut &str) -> Option<String> {
    if !s.starts_with('"') {
        return None;
    }
    let end = s[1..].find('"')?;
    let card = s[1..1 + end].to_string();
    *s = s[2 + end..].trim_start();
    Some(card)
}

fn decode_relationship(op: &str) -> Option<RelationshipStyle> {
    let op = op.trim();
    if op.is_empty() {
        return None;
    }

    let dot_count = op.chars().filter(|c| *c == '.').count();
    let dash_count = op.chars().filter(|c| *c == '-').count();
    let line = if dot_count > 0 && dot_count >= dash_count {
        LineKind::Dashed
    } else {
        LineKind::Solid
    };

    let mut head = EdgeEnd::None;
    let mut tail = EdgeEnd::None;

    if op.starts_with("<|") {
        tail = EdgeEnd::HollowArrow;
    } else if op.starts_with('*') {
        tail = EdgeEnd::FilledDiamond;
    } else if op.starts_with('o') {
        tail = EdgeEnd::HollowDiamond;
    } else if op.starts_with('<') {
        tail = EdgeEnd::Arrow;
    }

    if op.ends_with("|>") {
        head = EdgeEnd::HollowArrow;
    } else if op.ends_with('>') {
        head = EdgeEnd::Arrow;
    } else if op.ends_with('*') {
        head = EdgeEnd::FilledDiamond;
    } else if op.ends_with('o') {
        head = EdgeEnd::HollowDiamond;
    }

    Some(RelationshipStyle { line, head, tail })
}

// ───── Layout & rendering ─────

fn build_flowchart_graph(nodes: &[ClassNode], edges: &[ClassEdge]) -> FlGraph {
    use super::flowchart::Direction;
    let mut graph_nodes = HashMap::new();
    let mut node_order = Vec::new();
    for n in nodes {
        graph_nodes.insert(
            n.name.clone(),
            FlNode {
                label: n.name.clone(),
                shape: super::super::canvas::NodeShape::Rectangle,
            },
        );
        node_order.push(n.name.clone());
    }
    let graph_edges: Vec<FlEdge> = edges
        .iter()
        .map(|e| FlEdge {
            from: e.from.clone(),
            to: e.to.clone(),
            label: None,
        })
        .collect();
    FlGraph {
        direction: Direction::TopDown,
        nodes: graph_nodes,
        edges: graph_edges,
        node_order,
    }
}

fn compute_card_width(node: &ClassNode) -> usize {
    let title_len = node.title.chars().count();
    let stereo_len = node
        .stereotype
        .as_ref()
        .map(|s| s.chars().count())
        .unwrap_or(0);
    let max_member = node
        .members
        .iter()
        .map(|m| m.text.chars().count())
        .max()
        .unwrap_or(0);
    // draw_card geometry: title needs `title_len + 5` columns; each member row
    // (key=vis char, value=text) needs `text_len + 6` columns to avoid clipping.
    let w_for_title = title_len + 5;
    let w_for_stereo = stereo_len + 6;
    let w_for_member = max_member + 6;
    w_for_title.max(w_for_stereo).max(w_for_member).max(7)
}

fn compute_card_height(node: &ClassNode) -> usize {
    let content_rows = node.stereotype.is_some() as usize + node.members.len();
    2 + content_rows
}

fn visibility_color(c: char, theme: &Theme) -> Color {
    match c {
        '+' => theme.member_plus,
        '-' => theme.member_minus,
        '#' => theme.member_hash,
        '~' => theme.member_tilde,
        _ => theme.fg,
    }
}

fn build_card_rows(node: &ClassNode, theme: &Theme) -> Vec<CardDrawRow> {
    let mut rows = Vec::with_capacity(node.members.len() + 1);
    if let Some(stereo) = &node.stereotype {
        rows.push(CardDrawRow {
            key: String::new(),
            value_text: stereo.clone(),
            value_color: Some(theme.title),
            key_color: None,
            is_connector: false,
        });
    }
    for m in &node.members {
        let color = visibility_color(m.visibility, theme);
        rows.push(CardDrawRow {
            key: m.visibility.to_string(),
            value_text: m.text.clone(),
            value_color: Some(theme.fg),
            key_color: Some(color),
            is_connector: false,
        });
    }
    rows
}

fn render_class(
    nodes: &[ClassNode],
    edges: &[ClassEdge],
    theme: &Theme,
) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let graph = build_flowchart_graph(nodes, edges);
    let mut layers = assign_layers(&graph);
    order_within_layers(&mut layers, &graph);

    let h_gap = 4usize;
    let edge_gap = 4usize;

    let mut widths: HashMap<String, usize> = HashMap::new();
    let mut heights: HashMap<String, usize> = HashMap::new();
    for n in nodes {
        widths.insert(n.name.clone(), compute_card_width(n));
        heights.insert(n.name.clone(), compute_card_height(n));
    }

    let mut max_layer_width = 0usize;
    for layer in &layers {
        let lw: usize = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .sum::<usize>()
            + layer.len().saturating_sub(1) * h_gap;
        max_layer_width = max_layer_width.max(lw);
    }
    let canvas_width = max_layer_width + 6;

    let mut layer_ys: Vec<usize> = Vec::with_capacity(layers.len());
    let mut y = 0usize;
    for layer in &layers {
        layer_ys.push(y);
        let max_h = layer
            .iter()
            .map(|id| heights.get(id).copied().unwrap_or(3))
            .max()
            .unwrap_or(3);
        y += max_h + edge_gap;
    }
    let canvas_height = y.saturating_sub(edge_gap);
    if canvas_height == 0 {
        return None;
    }

    let mut canvas = Canvas::new(canvas_width, canvas_height);
    let border_fg = Some(theme.code_border);
    let title_fg = Some(theme.fg);
    let key_fg = Some(theme.fg);
    let canvas_center = canvas_width / 2;

    // (center_x, top_y, bottom_y, width) per class name.
    let mut card_info: HashMap<String, (usize, usize, usize, usize)> = HashMap::new();

    for (layer_idx, layer) in layers.iter().enumerate() {
        let y_base = layer_ys[layer_idx];

        let node_widths: Vec<usize> = layer
            .iter()
            .map(|id| widths.get(id).copied().unwrap_or(7))
            .collect();
        let layer_width: usize =
            node_widths.iter().sum::<usize>() + layer.len().saturating_sub(1) * h_gap;

        let mut centers_in_layer: Vec<usize> = Vec::new();
        let mut cumulative = 0usize;
        for &w in &node_widths {
            centers_in_layer.push(cumulative + w / 2);
            cumulative += w + h_gap;
        }
        let layer_center = if layer_width > 0 { layer_width / 2 } else { 0 };

        for (i, id) in layer.iter().enumerate() {
            let w = node_widths[i];
            let cx = (canvas_center as isize + centers_in_layer[i] as isize - layer_center as isize)
                .max(w as isize / 2) as usize;
            let left_x = cx.saturating_sub(w / 2);
            let h = heights.get(id).copied().unwrap_or(3);

            if let Some(node) = nodes.iter().find(|n| &n.name == id) {
                let rows = build_card_rows(node, theme);
                canvas.draw_card(
                    left_x,
                    y_base,
                    w,
                    &node.title,
                    &rows,
                    border_fg,
                    title_fg,
                    key_fg,
                    &HashSet::new(),
                    None,
                    None,
                );
            }

            card_info.insert(id.clone(), (cx, y_base, y_base + h - 1, w));
        }
    }

    for (edge_idx, edge) in edges.iter().enumerate() {
        let (Some(src), Some(dst)) = (card_info.get(&edge.from), card_info.get(&edge.to)) else {
            continue;
        };
        let (src_cx, _src_top, src_bottom, _src_w) = *src;
        let (dst_cx, dst_top, _dst_bot, _dst_w) = *dst;
        let edge_fg = Some(edge_color(theme, edge_idx));

        let far_label: Option<String> = match (&edge.cardinalities.0, &edge.cardinalities.1) {
            (Some(a), Some(b)) => Some(format!("\"{a}\" .. \"{b}\"")),
            (Some(a), None) => Some(format!("\"{a}\"")),
            (None, Some(b)) => Some(format!("\"{b}\"")),
            (None, None) => None,
        };

        canvas.draw_edge_td(
            src_cx,
            src_bottom,
            dst_cx,
            dst_top,
            EdgeStyle {
                dashed: matches!(edge.style.line, LineKind::Dashed),
                head: edge.style.head,
                tail: edge.style.tail,
                label: edge.label.as_deref(),
                far_label: far_label.as_deref(),
            },
            edge_fg,
        );
    }

    let rows = canvas.to_span_rows(theme);
    Some((rows, canvas_width))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::StyledSpan;
    use crate::theme::Theme;

    fn row_text(row: &[StyledSpan]) -> String {
        row.iter().map(|span| span.text.as_str()).collect()
    }

    fn render_text(input: &str, theme: &Theme) -> String {
        let (rows, _w) = render(input, theme).expect("expected class diagram to render");
        rows.iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_text_full(input: &str) -> String {
        let theme = Theme::dark();
        render_text(input, &theme)
    }

    // ───── Parser tests ─────

    #[test]
    fn parses_bare_class() {
        let (nodes, _edges) = parse_class_diagram("classDiagram\nclass Foo").unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "Foo");
        assert_eq!(nodes[0].title, "Foo");
        assert!(nodes[0].members.is_empty());
        assert!(nodes[0].stereotype.is_none());
    }

    #[test]
    fn parses_class_with_body_and_visibility_markers() {
        let src = "classDiagram\nclass Foo {\n+publicAttr: Type\n-privateAttr: Type\n#protectedAttr: Type\n~packageAttr: Type\n+method(): ReturnType\n}";
        let (nodes, _edges) = parse_class_diagram(src).unwrap();
        assert_eq!(nodes.len(), 1);
        let members = &nodes[0].members;
        assert_eq!(members.len(), 5);
        assert_eq!(members[0].visibility, '+');
        assert_eq!(members[0].text, "publicAttr: Type");
        assert_eq!(members[1].visibility, '-');
        assert_eq!(members[1].text, "privateAttr: Type");
        assert_eq!(members[2].visibility, '#');
        assert_eq!(members[2].text, "protectedAttr: Type");
        assert_eq!(members[3].visibility, '~');
        assert_eq!(members[3].text, "packageAttr: Type");
        assert_eq!(members[4].visibility, '+');
        assert_eq!(members[4].text, "method(): ReturnType");
    }

    #[test]
    fn strips_generics_for_layout_key_keeps_them_in_title() {
        let (nodes, _edges) = parse_class_diagram("classDiagram\nclass Foo<T, K>").unwrap();
        assert_eq!(
            nodes[0].name, "Foo",
            "base name used for relationship lookup"
        );
        assert_eq!(nodes[0].title, "Foo<T, K>", "title preserves generics");
    }

    #[test]
    fn parses_inline_stereotype_bracket_form() {
        let (nodes, _edges) =
            parse_class_diagram("classDiagram\nclass Foo [<<interface>>]").unwrap();
        assert_eq!(nodes[0].stereotype.as_deref(), Some("<<interface>>"));
    }

    #[test]
    fn parses_stereotype_inside_body() {
        let src = "classDiagram\nclass Foo {\n<<interface>>\n+method(): void\n}";
        let (nodes, _edges) = parse_class_diagram(src).unwrap();
        assert_eq!(nodes[0].stereotype.as_deref(), Some("<<interface>>"));
        assert_eq!(
            nodes[0].members.len(),
            1,
            "stereotype line not counted as a member"
        );
        assert_eq!(nodes[0].members[0].text, "method(): void");
    }

    #[test]
    fn decodes_inheritance_operator_left_arrow() {
        let style = decode_relationship("<|--").unwrap();
        assert_eq!(style.line, LineKind::Solid);
        assert_eq!(style.tail, EdgeEnd::HollowArrow);
        assert_eq!(style.head, EdgeEnd::None);
    }

    #[test]
    fn decodes_inheritance_operator_right_arrow() {
        let style = decode_relationship("--|>").unwrap();
        assert_eq!(style.line, LineKind::Solid);
        assert_eq!(style.head, EdgeEnd::HollowArrow);
        assert_eq!(style.tail, EdgeEnd::None);
    }

    #[test]
    fn decodes_composition_operator() {
        let style = decode_relationship("*--").unwrap();
        assert_eq!(style.line, LineKind::Solid);
        assert_eq!(style.tail, EdgeEnd::FilledDiamond);
        assert_eq!(style.head, EdgeEnd::None);
    }

    #[test]
    fn decodes_aggregation_operator() {
        let style = decode_relationship("o--").unwrap();
        assert_eq!(style.line, LineKind::Solid);
        assert_eq!(style.tail, EdgeEnd::HollowDiamond);
        assert_eq!(style.head, EdgeEnd::None);
    }

    #[test]
    fn decodes_association_operator() {
        let style = decode_relationship("-->").unwrap();
        assert_eq!(style.line, LineKind::Solid);
        assert_eq!(style.head, EdgeEnd::Arrow);
        assert_eq!(style.tail, EdgeEnd::None);
    }

    #[test]
    fn decodes_dependency_operator_dashed() {
        let style = decode_relationship("..>").unwrap();
        assert_eq!(style.line, LineKind::Dashed);
        assert_eq!(style.head, EdgeEnd::Arrow);
        assert_eq!(style.tail, EdgeEnd::None);
    }

    #[test]
    fn decodes_realization_operator_dashed() {
        let style = decode_relationship("<|..").unwrap();
        assert_eq!(style.line, LineKind::Dashed);
        assert_eq!(style.tail, EdgeEnd::HollowArrow);
        assert_eq!(style.head, EdgeEnd::None);
    }

    #[test]
    fn parses_cardinality_quotes_and_label() {
        let edge = parse_relationship("Animal \"1\" --> \"0..n\" Dog : owns").unwrap();
        assert_eq!(edge.from, "Animal");
        assert_eq!(edge.to, "Dog");
        assert_eq!(edge.cardinalities.0.as_deref(), Some("1"));
        assert_eq!(edge.cardinalities.1.as_deref(), Some("0..n"));
        assert_eq!(edge.label.as_deref(), Some("owns"));
        assert_eq!(edge.style.head, EdgeEnd::Arrow);
    }

    #[test]
    fn returns_none_for_missing_header() {
        assert!(parse_class_diagram("class Foo").is_none());
    }

    #[test]
    fn returns_none_for_empty_body() {
        assert!(parse_class_diagram("classDiagram").is_none());
    }

    // ───── Renderer smoke tests ─────

    #[test]
    fn renders_class_name_and_member_rows() {
        let out = render_text_full("classDiagram\nclass Foo {\n+attr: int\n-method(): void\n}");
        assert!(out.contains("Foo"), "class name should appear: {out}");
        assert!(
            out.contains("attr: int"),
            "public member should appear: {out}"
        );
        assert!(
            out.contains("method(): void"),
            "private member should appear: {out}"
        );
    }

    #[test]
    fn renders_stereotype_row_above_name_in_card() {
        // The stereotype appears as the first content row inside the card,
        // directly below the title bar.
        let out = render_text_full("classDiagram\nclass Foo [<<interface>>]");
        assert!(out.contains("Foo"), "title row should appear");
        assert!(
            out.contains("<<interface>>"),
            "stereotype text should appear"
        );
    }

    #[test]
    fn inheritance_edge_renders_hollow_arrow_glyph() {
        // TD layout: source (parent) at top with `△` tail glyph.
        let out = render_text_full("classDiagram\nAnimal <|-- Dog");
        assert!(
            out.contains('△') || out.contains('▽') || out.contains('▷') || out.contains('◁'),
            "inheritance should render a hollow-arrow glyph; got:\n{out}"
        );
    }

    #[test]
    fn composition_edge_renders_filled_diamond() {
        let out = render_text_full("classDiagram\nCar *-- Wheel");
        assert!(
            out.contains('◆'),
            "composition should render ◆; got:\n{out}"
        );
    }

    #[test]
    fn aggregation_edge_renders_hollow_diamond() {
        let out = render_text_full("classDiagram\nLibrary o-- Book");
        assert!(
            out.contains('◇'),
            "aggregation should render ◇; got:\n{out}"
        );
    }

    #[test]
    fn dependency_edge_is_dashed() {
        // `..>` is dashed; in TD layout the vertical body uses `┊` and any
        // horizontal segment uses `┄`. With distinct source/dest columns we
        // exercise the bent path (horizontal `┄`).
        let out = render_text_full("classDiagram\nA ..> B : uses");
        assert!(
            out.contains('┊') || out.contains('┄'),
            "dependency should render a dashed body glyph; got:\n{out}"
        );
    }

    // ───── Layout regression ─────

    #[test]
    fn canvas_has_positive_dimensions_and_places_class_name() {
        let theme = Theme::dark();
        let (rows, width) =
            render("classDiagram\nclass Foo {\n+attr: int\n}", &theme).expect("render ok");
        assert!(width > 0, "canvas width should be positive");
        assert!(!rows.is_empty(), "should produce at least one row");
        let joined: String = rows
            .iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("Foo"),
            "class name should be placed somewhere"
        );
    }

    #[test]
    fn dispatcher_routes_classdiagram_through_native_renderer() {
        use crate::diagram::render_mermaid;
        let theme = Theme::dark();
        let result = render_mermaid("classDiagram\nclass A\nclass B\nA --> B", &theme);
        assert!(result.is_ok(), "dispatcher should succeed for classDiagram");
        let (rows, _w) = result.unwrap();
        let joined: String = rows
            .iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains('A') && joined.contains('B'));
    }
}
