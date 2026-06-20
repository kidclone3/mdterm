use std::collections::{HashMap, HashSet};

use crate::style::StyledSpan;
use crate::theme::Theme;

use super::super::canvas::{Canvas, Card, CardDrawRow, CrowDir, EdgeEnd, EdgeStyle};
use super::super::theme::edge_color;
use super::flowchart::{Edge as FlEdge, Graph as FlGraph, Node as FlNode};
use super::{assign_layers, order_within_layers};

// ───── Data types ─────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyKind {
    Pk,
    Fk,
}

#[derive(Debug, Clone)]
struct ErField {
    type_name: String,
    name: String,
    key: Option<KeyKind>,
    comment: Option<String>,
}

#[derive(Debug, Clone)]
struct ErEntity {
    name: String,
    fields: Vec<ErField>,
}

#[derive(Debug, Clone)]
struct ErRel {
    from: String,
    from_card: Card,
    to: String,
    to_card: Card,
    dashed: bool,
    label: Option<String>,
}

// ───── Public entry ─────

pub(crate) fn render(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let (entities, rels) = parse_er_diagram(code)?;
    render_er(&entities, &rels, theme)
}

// ───── Parser ─────

fn parse_er_diagram(code: &str) -> Option<(Vec<ErEntity>, Vec<ErRel>)> {
    let mut entities: HashMap<String, ErEntity> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut rels: Vec<ErRel> = Vec::new();

    let mut lines = code.lines().peekable();
    let mut header_seen = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        if !header_seen {
            if trimmed == "erDiagram" {
                header_seen = true;
                continue;
            } else {
                return None;
            }
        }

        if trimmed.starts_with("direction ")
            || trimmed.starts_with("skinparam")
            || trimmed.starts_with("style ")
        {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("entity ") {
            let (name, has_body) = parse_entity_header(rest)?;
            let fields = if has_body {
                let mut body: Vec<ErField> = Vec::new();
                for body_line in lines.by_ref() {
                    let bt = body_line.trim();
                    if bt == "}" {
                        break;
                    }
                    if bt.is_empty() {
                        continue;
                    }
                    if let Some(f) = parse_entity_field(bt) {
                        body.push(f);
                    }
                }
                body
            } else {
                Vec::new()
            };

            if entities.contains_key(&name) {
                entities.get_mut(&name).unwrap().fields = fields;
            } else {
                entities.insert(
                    name.clone(),
                    ErEntity {
                        name: name.clone(),
                        fields,
                    },
                );
                order.push(name);
            }
            continue;
        }

        if let Some(rel) = parse_relationship(trimmed) {
            for name in [&rel.from, &rel.to] {
                if !entities.contains_key(name) {
                    entities.insert(
                        name.clone(),
                        ErEntity {
                            name: name.clone(),
                            fields: Vec::new(),
                        },
                    );
                    order.push(name.clone());
                }
            }
            rels.push(rel);
            continue;
        }
    }

    if entities.is_empty() {
        return None;
    }

    let entities_vec = order
        .into_iter()
        .filter_map(|n| entities.remove(&n))
        .collect();
    Some((entities_vec, rels))
}

/// Parse the text after `entity `: identifier and optional `{` body marker.
fn parse_entity_header(rest: &str) -> Option<(String, bool)> {
    let rest = rest.trim();
    let name_end = rest
        .find(|c: char| c.is_whitespace() || c == '{')
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let name = rest[..name_end].to_string();
    let after = rest[name_end..].trim();
    let has_body = after.starts_with('{');
    Some((name, has_body))
}

/// Parse a single field line: `type name [PK|FK] ["comment"]`.
fn parse_entity_field(line: &str) -> Option<ErField> {
    let rest = line.trim();
    if rest.is_empty() {
        return None;
    }

    let mut comment = None;
    let mut core = rest;
    if let Some(quote_start) = rest.find('"')
        && let Some(quote_end) = rest[quote_start + 1..].find('"')
    {
        let end = quote_start + 1 + quote_end;
        comment = Some(rest[quote_start + 1..end].to_string());
        core = rest[..quote_start].trim_end();
    }

    let mut tokens = core.split_whitespace();
    let type_name = tokens.next()?.to_string();
    let name = tokens.next()?.to_string();

    let key = match tokens.next() {
        Some("PK") => Some(KeyKind::Pk),
        Some("FK") => Some(KeyKind::Fk),
        _ => None,
    };

    Some(ErField {
        type_name,
        name,
        key,
        comment,
    })
}

/// Parse `FROM [left-card][joiner][right-card] TO [: label]`.
/// Left cardinality tokens: `||`, `|o`, `}o`, `}|`. Right: `o|`, `o{`, `|{`.
/// Joiner: `--` (solid) or `..` (dashed).
fn parse_relationship(line: &str) -> Option<ErRel> {
    let s = line.trim();

    let first_end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(s.len());
    if first_end == 0 {
        return None;
    }
    let from = s[..first_end].to_string();
    let mut rest = s[first_end..].trim_start();

    let op_end = rest
        .find(|c: char| !matches!(c, '|' | 'o' | '}' | '{' | '-' | '.'))
        .unwrap_or(rest.len());
    if op_end < 6 {
        return None;
    }
    let op = &rest[..op_end];
    rest = rest[op_end..].trim_start();

    let second_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if second_end == 0 {
        return None;
    }
    let to = rest[..second_end].to_string();
    rest = rest[second_end..].trim_start();

    let label = rest
        .strip_prefix(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if op.len() != 6 {
        return None;
    }
    let left = &op[..2];
    let joiner = &op[2..4];
    let right = &op[4..6];

    let from_card = decode_left_card(left)?;
    let to_card = decode_right_card(right)?;
    let dashed = joiner == "..";
    if joiner != "--" && !dashed {
        return None;
    }

    Some(ErRel {
        from,
        from_card,
        to,
        to_card,
        dashed,
        label,
    })
}

fn decode_left_card(token: &str) -> Option<Card> {
    match token {
        "||" => Some(Card::One),
        "|o" => Some(Card::ZeroOrOne),
        "}o" => Some(Card::ZeroOrMany),
        "}|" => Some(Card::OneOrMany),
        _ => None,
    }
}

fn decode_right_card(token: &str) -> Option<Card> {
    match token {
        "||" => Some(Card::One),
        "o|" => Some(Card::ZeroOrOne),
        "o{" => Some(Card::ZeroOrMany),
        "|{" => Some(Card::OneOrMany),
        _ => None,
    }
}

// ───── Layout & rendering ─────

fn build_flowchart_graph(entities: &[ErEntity], rels: &[ErRel]) -> FlGraph {
    use super::flowchart::Direction;
    let mut nodes = HashMap::new();
    let mut node_order = Vec::new();
    for e in entities {
        nodes.insert(
            e.name.clone(),
            FlNode {
                label: e.name.clone(),
                shape: super::super::canvas::NodeShape::Rectangle,
            },
        );
        node_order.push(e.name.clone());
    }
    let edges: Vec<FlEdge> = rels
        .iter()
        .map(|r| FlEdge {
            from: r.from.clone(),
            to: r.to.clone(),
            label: None,
        })
        .collect();
    FlGraph {
        direction: Direction::TopDown,
        nodes,
        edges,
        node_order,
    }
}

fn compute_card_width(entity: &ErEntity) -> usize {
    let title_len = entity.name.chars().count();
    let has_key = entity.fields.iter().any(|f| f.key.is_some());
    let key_col = if has_key { 2 } else { 0 };
    let max_field = entity
        .fields
        .iter()
        .map(|f| {
            let base = f.type_name.chars().count() + 1 + f.name.chars().count();
            match &f.comment {
                Some(c) => base + 1 + c.chars().count(),
                None => base,
            }
        })
        .max()
        .unwrap_or(0);
    let w_for_title = title_len + 5;
    let w_for_field = max_field + key_col + 5;
    w_for_title.max(w_for_field).max(7)
}

fn compute_card_height(entity: &ErEntity) -> usize {
    2 + entity.fields.len()
}

fn build_card_rows(entity: &ErEntity, theme: &Theme) -> Vec<CardDrawRow> {
    let mut rows = Vec::with_capacity(entity.fields.len());
    for f in &entity.fields {
        let (key_text, key_color) = match f.key {
            Some(KeyKind::Pk) => ("PK".to_string(), Some(theme.key_badge_pk)),
            Some(KeyKind::Fk) => ("FK".to_string(), Some(theme.key_badge_fk)),
            None => (String::new(), None),
        };
        let value_text = match &f.comment {
            Some(c) => format!("{} {} {}", f.type_name, f.name, c),
            None => format!("{} {}", f.type_name, f.name),
        };
        rows.push(CardDrawRow {
            key: key_text,
            value_text,
            value_color: Some(theme.fg),
            key_color,
            is_connector: false,
        });
    }
    rows
}

#[allow(clippy::too_many_lines)]
fn render_er(
    entities: &[ErEntity],
    rels: &[ErRel],
    theme: &Theme,
) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let graph = build_flowchart_graph(entities, rels);
    let mut layers = assign_layers(&graph);
    order_within_layers(&mut layers, &graph);

    let h_gap = 4usize;
    let edge_gap = 4usize;

    let mut widths: HashMap<String, usize> = HashMap::new();
    let mut heights: HashMap<String, usize> = HashMap::new();
    for e in entities {
        widths.insert(e.name.clone(), compute_card_width(e));
        heights.insert(e.name.clone(), compute_card_height(e));
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
            let cx = (canvas_center as isize + centers_in_layer[i] as isize
                - layer_center as isize)
                .max(w as isize / 2) as usize;
            let left_x = cx.saturating_sub(w / 2);
            let h = heights.get(id).copied().unwrap_or(3);

            if let Some(entity) = entities.iter().find(|e| &e.name == id) {
                let rows = build_card_rows(entity, theme);
                canvas.draw_card(
                    left_x,
                    y_base,
                    w,
                    &entity.name,
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

    for (edge_idx, rel) in rels.iter().enumerate() {
        let (Some(src), Some(dst)) = (card_info.get(&rel.from), card_info.get(&rel.to)) else {
            continue;
        };
        let (src_cx, _src_top, src_bottom, _src_w) = *src;
        let (dst_cx, dst_top, _dst_bot, _dst_w) = *dst;
        let edge_fg = Some(edge_color(theme, edge_idx));

        canvas.draw_edge_td(
            src_cx,
            src_bottom,
            dst_cx,
            dst_top,
            EdgeStyle {
                dashed: rel.dashed,
                head: EdgeEnd::None,
                tail: EdgeEnd::None,
                label: rel.label.as_deref(),
                far_label: None,
            },
            edge_fg,
        );

        if src_bottom + 1 < dst_top {
            canvas.draw_crowsfoot(src_cx, src_bottom + 1, CrowDir::Down, rel.from_card, edge_fg);
            if dst_top > 0 {
                canvas.draw_crowsfoot(dst_cx, dst_top - 1, CrowDir::Up, rel.to_card, edge_fg);
            }
        }
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
        let (rows, _w) = render(input, theme).expect("expected er diagram to render");
        rows.iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_text_full(input: &str) -> String {
        render_text(input, &Theme::dark())
    }

    fn row_spans(input: &str, theme: &Theme) -> Vec<Vec<StyledSpan>> {
        let (rows, _w) = render(input, theme).expect("expected er diagram to render");
        rows
    }

    // ───── Parser: entity ─────

    #[test]
    fn parses_entity_with_body() {
        let src = "erDiagram\nentity CUSTOMER {\nbigint id PK\nstring name\ntimestamp created_at \"created timestamp\"\n}";
        let (entities, _rels) = parse_er_diagram(src).unwrap();
        assert_eq!(entities.len(), 1);
        let e = &entities[0];
        assert_eq!(e.name, "CUSTOMER");
        assert_eq!(e.fields.len(), 3);

        assert_eq!(e.fields[0].type_name, "bigint");
        assert_eq!(e.fields[0].name, "id");
        assert_eq!(e.fields[0].key, Some(KeyKind::Pk));
        assert!(e.fields[0].comment.is_none());

        assert_eq!(e.fields[1].type_name, "string");
        assert_eq!(e.fields[1].name, "name");
        assert!(e.fields[1].key.is_none());

        assert_eq!(e.fields[2].type_name, "timestamp");
        assert_eq!(e.fields[2].name, "created_at");
        assert_eq!(
            e.fields[2].comment.as_deref(),
            Some("created timestamp"),
            "quoted trailing comment should be extracted"
        );
    }

    #[test]
    fn parses_fk_key_kind() {
        let src = "erDiagram\nentity ORDER {\nbigint customer_id FK\n}";
        let (entities, _) = parse_er_diagram(src).unwrap();
        assert_eq!(entities[0].fields[0].key, Some(KeyKind::Fk));
    }

    #[test]
    fn parses_standalone_entity_without_body() {
        let (entities, _) = parse_er_diagram("erDiagram\nentity CUSTOMER").unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].fields.is_empty());
    }

    #[test]
    fn returns_none_for_missing_header() {
        assert!(parse_er_diagram("entity CUSTOMER").is_none());
    }

    #[test]
    fn returns_none_for_empty_body() {
        assert!(parse_er_diagram("erDiagram").is_none());
    }

    // ───── Parser: crow's-foot decoding ─────

    #[test]
    fn decodes_left_card_tokens() {
        assert_eq!(decode_left_card("||"), Some(Card::One));
        assert_eq!(decode_left_card("|o"), Some(Card::ZeroOrOne));
        assert_eq!(decode_left_card("}o"), Some(Card::ZeroOrMany));
        assert_eq!(decode_left_card("}|"), Some(Card::OneOrMany));
        assert!(decode_left_card("xx").is_none());
    }

    #[test]
    fn decodes_right_card_tokens() {
        assert_eq!(decode_right_card("||"), Some(Card::One));
        assert_eq!(decode_right_card("o|"), Some(Card::ZeroOrOne));
        assert_eq!(decode_right_card("o{"), Some(Card::ZeroOrMany));
        assert_eq!(decode_right_card("|{"), Some(Card::OneOrMany));
        assert!(decode_right_card("xx").is_none());
    }

    #[test]
    fn parses_relationship_each_crow_token() {
        let cases: &[(&str, Card, Card)] = &[
            ("A ||--|| B", Card::One, Card::One),
            ("A ||--o{ B", Card::One, Card::ZeroOrMany),
            ("A ||--|{ B", Card::One, Card::OneOrMany),
            ("A }o--|| B", Card::ZeroOrMany, Card::One),
            ("A }|--o| B", Card::OneOrMany, Card::ZeroOrOne),
            ("A |o--o{ B", Card::ZeroOrOne, Card::ZeroOrMany),
            ("A }o--o{ B : has", Card::ZeroOrMany, Card::ZeroOrMany),
        ];
        for (line, want_from, want_to) in cases {
            let rel = parse_relationship(line).unwrap_or_else(|| panic!("failed: {line}"));
            assert_eq!(rel.from, "A", "line: {line}");
            assert_eq!(rel.to, "B", "line: {line}");
            assert_eq!(rel.from_card, *want_from, "from_card for {line}");
            assert_eq!(rel.to_card, *want_to, "to_card for {line}");
            assert!(!rel.dashed, "should be solid: {line}");
        }
    }

    #[test]
    fn parses_dashed_relationship_and_label() {
        let rel = parse_relationship("CUSTOMER }o..o{ ORDER : places").unwrap();
        assert_eq!(rel.from, "CUSTOMER");
        assert_eq!(rel.to, "ORDER");
        assert!(rel.dashed, "'..' joiner means dashed");
        assert_eq!(rel.label.as_deref(), Some("places"));
    }

    #[test]
    fn rejects_malformed_operator() {
        assert!(parse_relationship("A --> B").is_none(), "flowchart arrow is not ER syntax");
        assert!(parse_relationship("A |-| B").is_none(), "bad joiner");
    }

    // ───── Renderer smoke tests ─────

    #[test]
    fn renders_entity_name_and_field_rows() {
        let out = render_text_full(
            "erDiagram\nentity CUSTOMER {\nbigint id PK\nstring name\n}",
        );
        assert!(out.contains("CUSTOMER"), "entity name should appear: {out}");
        assert!(out.contains("bigint"), "field type should appear: {out}");
        assert!(out.contains("id"), "field name should appear: {out}");
        assert!(out.contains("string"), "second field type should appear: {out}");
    }

    #[test]
    fn renders_pk_badge_in_yellow() {
        let theme = Theme::dark();
        let spans = row_spans(
            "erDiagram\nentity CUSTOMER {\nbigint id PK\n}",
            &theme,
        );
        let flat: String = spans
            .iter()
            .flat_map(|r| r.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(flat.contains("PK"), "PK badge text should appear: {flat}");

        let pk_yellow = spans.iter().any(|row| {
            row.iter().any(|s| {
                s.text.contains("PK") && s.style.fg == Some(theme.key_badge_pk)
            })
        });
        assert!(pk_yellow, "PK badge should use key_badge_pk color");
    }

    #[test]
    fn renders_fk_badge_in_blue() {
        let theme = Theme::dark();
        let spans = row_spans(
            "erDiagram\nentity ORDER {\nbigint customer_id FK\n}",
            &theme,
        );
        let flat: String = spans
            .iter()
            .flat_map(|r| r.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(flat.contains("FK"), "FK badge text should appear: {flat}");

        let fk_blue = spans.iter().any(|row| {
            row.iter().any(|s| {
                s.text.contains("FK") && s.style.fg == Some(theme.key_badge_fk)
            })
        });
        assert!(fk_blue, "FK badge should use key_badge_fk color");
    }

    #[test]
    fn renders_crowsfoot_glyph_at_endpoint() {
        let out = render_text_full("erDiagram\nCUSTOMER ||--o{ ORDER : places");
        let has_crow = ['│', '|', 'o', '⟨', '⟩']
            .iter()
            .any(|c| out.contains(*c));
        assert!(
            has_crow,
            "a crow's-foot glyph should appear at an endpoint: {out}"
        );
    }

    #[test]
    fn renders_relationship_label_on_edge() {
        let out = render_text_full("erDiagram\nCUSTOMER ||--o{ ORDER : places");
        assert!(
            out.contains("places"),
            "relationship label should appear: {out}"
        );
    }

    // ───── Layout regression ─────

    #[test]
    fn canvas_has_positive_dimensions_and_places_entity_name() {
        let theme = Theme::dark();
        let (rows, width) = render(
            "erDiagram\nentity CUSTOMER {\nbigint id PK\n}",
            &theme,
        )
        .expect("render ok");
        assert!(width > 0, "canvas width should be positive");
        assert!(!rows.is_empty(), "should produce at least one row");
        let joined: String = rows.iter().map(|r| row_text(r)).collect::<Vec<_>>().join("\n");
        assert!(
            joined.contains("CUSTOMER"),
            "entity name should be placed somewhere"
        );
    }

    #[test]
    fn dispatcher_routes_erdiagram_through_native_renderer() {
        use crate::diagram::render_mermaid;
        let theme = Theme::dark();
        let result = render_mermaid(
            "erDiagram\nCUSTOMER ||--o{ ORDER : places",
            &theme,
        );
        assert!(result.is_ok(), "dispatcher should succeed for erDiagram");
        let (rows, _w) = result.unwrap();
        let joined: String = rows.iter().map(|r| row_text(r)).collect::<Vec<_>>().join("\n");
        assert!(joined.contains("CUSTOMER") && joined.contains("ORDER"));
    }
}
