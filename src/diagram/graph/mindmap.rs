use crate::style::StyledSpan;
use crate::theme::Theme;

use super::super::canvas::{Canvas, NodeShape};
use super::super::theme::edge_color;
use super::flowchart::parse_node_ref;
use super::label_box_width;

use std::collections::HashMap;

const NODE_HEIGHT: usize = 3;
const SIBLING_GAP: usize = 1;
const COL_GAP: usize = 4;
const MARGIN: usize = 2;

struct MindNode {
    label: String,
    shape: NodeShape,
    children: Vec<MindNode>,
}

pub(crate) fn render(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let root = parse_mindmap(code)?;
    let canvas = draw_tree(&root, theme);
    let width = canvas.width;
    let rows = canvas.to_span_rows(theme);
    Some((rows, width))
}

// ───── Parser ─────

fn parse_mindmap(code: &str) -> Option<MindNode> {
    let mut header_seen = false;
    let mut body: Vec<(usize, &str)> = Vec::new();

    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        if !header_seen {
            if trimmed == "mindmap" || trimmed.starts_with("mindmap ") {
                header_seen = true;
                continue;
            }
            return None;
        }
        if trimmed.starts_with("classDef ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("style ")
        {
            continue;
        }
        let cleaned = if let Some(idx) = trimmed.find(":::") {
            trimmed[..idx].trim_end()
        } else {
            trimmed
        };
        if cleaned.is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        body.push((indent, cleaned));
    }

    if !header_seen || body.is_empty() {
        return None;
    }

    let baseline = body[0].0;
    let unit = if body.len() > 1 {
        body[1].0.saturating_sub(baseline).max(1)
    } else {
        1
    };

    let entries: Vec<(usize, &str)> = body
        .iter()
        .map(|(indent, text)| (indent.saturating_sub(baseline) / unit, *text))
        .collect();

    let mut idx = 0;
    let root = build_node(&entries, &mut idx, 0)?;
    Some(root)
}

fn build_node(
    entries: &[(usize, &str)],
    idx: &mut usize,
    current_depth: usize,
) -> Option<MindNode> {
    if *idx >= entries.len() {
        return None;
    }
    let (depth, text) = entries[*idx];
    if depth != current_depth {
        return None;
    }

    let (label, shape) = parse_label_shape(text);
    let mut node = MindNode {
        label,
        shape,
        children: Vec::new(),
    };
    *idx += 1;

    while *idx < entries.len() {
        let (child_depth, _) = entries[*idx];
        if child_depth <= current_depth {
            break;
        }
        if child_depth == current_depth + 1 {
            if let Some(child) = build_node(entries, idx, current_depth + 1) {
                node.children.push(child);
            }
        } else {
            *idx += 1;
        }
    }

    Some(node)
}

fn parse_label_shape(text: &str) -> (String, NodeShape) {
    match parse_node_ref(text) {
        Some((_id, Some((label, shape)), _rest)) => (label, shape),
        _ => (text.trim().to_string(), NodeShape::Rectangle),
    }
}

// ───── Layout ─────

fn subtree_units(node: &MindNode) -> usize {
    if node.children.is_empty() {
        1
    } else {
        node.children
            .iter()
            .map(subtree_units)
            .sum::<usize>()
            .max(1)
    }
}

fn subtree_rows(node: &MindNode) -> usize {
    let units = subtree_units(node);
    units * NODE_HEIGHT + units.saturating_sub(1) * SIBLING_GAP
}

#[derive(Clone, Copy, Debug)]
struct Placed {
    cx: usize,
    cy: usize,
    width: usize,
}

fn collect_column_widths(node: &MindNode, signed_depth: isize, widths: &mut HashMap<isize, usize>) {
    let w = label_box_width(&node.label, node.shape);
    widths
        .entry(signed_depth)
        .and_modify(|w0| *w0 = (*w0).max(w))
        .or_insert(w);
    for child in &node.children {
        collect_column_widths(child, signed_depth + 1, widths);
    }
}

#[derive(Debug)]
struct Layout {
    placed: HashMap<*const MindNode, Placed>,
    edges: Vec<(*const MindNode, *const MindNode)>,
    #[allow(dead_code)]
    root_ptr: *const MindNode,
    canvas_width: usize,
    canvas_height: usize,
}

fn layout_tree(root: &MindNode) -> Layout {
    let mut right_children: Vec<&MindNode> = Vec::new();
    let mut left_children: Vec<&MindNode> = Vec::new();
    for (i, child) in root.children.iter().enumerate() {
        if i % 2 == 0 {
            right_children.push(child);
        } else {
            left_children.push(child);
        }
    }

    let mut col_widths: HashMap<isize, usize> = HashMap::new();
    col_widths.insert(0, label_box_width(&root.label, root.shape));
    for child in &right_children {
        collect_column_widths(child, 1, &mut col_widths);
    }
    for child in &left_children {
        collect_column_widths(child, -1, &mut col_widths);
    }

    let min_col = *col_widths.keys().min().unwrap_or(&0);
    let max_col = *col_widths.keys().max().unwrap_or(&0);

    let mut col_center_x: HashMap<isize, usize> = HashMap::new();
    let mut cursor = MARGIN;
    for c in min_col..=max_col {
        let w = col_widths.get(&c).copied().unwrap_or(0);
        col_center_x.insert(c, cursor + w / 2);
        cursor += w + COL_GAP;
    }
    let canvas_width = cursor + MARGIN - COL_GAP;

    let right_height: usize = right_children
        .iter()
        .map(|c| subtree_rows(c))
        .sum::<usize>()
        + right_children.len().saturating_sub(1) * SIBLING_GAP;
    let left_height: usize = left_children.iter().map(|c| subtree_rows(c)).sum::<usize>()
        + left_children.len().saturating_sub(1) * SIBLING_GAP;
    let canvas_height = right_height.max(left_height).max(NODE_HEIGHT);

    let root_ptr = root as *const MindNode;
    let mut placed: HashMap<*const MindNode, Placed> = HashMap::new();
    let mut edges: Vec<(*const MindNode, *const MindNode)> = Vec::new();

    let root_cx = *col_center_x.get(&0).unwrap_or(&MARGIN);
    let root_cy = canvas_height / 2;
    placed.insert(
        root_ptr,
        Placed {
            cx: root_cx,
            cy: root_cy,
            width: label_box_width(&root.label, root.shape),
        },
    );

    let mut top_y = canvas_height.saturating_sub(right_height) / 2;
    for child in &right_children {
        let rows = subtree_rows(child);
        layout_subtree(
            child,
            root_ptr,
            1,
            true,
            top_y,
            rows,
            &col_center_x,
            &mut placed,
            &mut edges,
        );
        top_y += rows + SIBLING_GAP;
    }

    let mut top_y = canvas_height.saturating_sub(left_height) / 2;
    for child in &left_children {
        let rows = subtree_rows(child);
        layout_subtree(
            child,
            root_ptr,
            -1,
            false,
            top_y,
            rows,
            &col_center_x,
            &mut placed,
            &mut edges,
        );
        top_y += rows + SIBLING_GAP;
    }

    Layout {
        placed,
        edges,
        root_ptr,
        canvas_width,
        canvas_height,
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_subtree(
    node: &MindNode,
    parent_ptr: *const MindNode,
    signed_depth: isize,
    going_right: bool,
    top_y: usize,
    rows: usize,
    col_center_x: &HashMap<isize, usize>,
    placed: &mut HashMap<*const MindNode, Placed>,
    edges: &mut Vec<(*const MindNode, *const MindNode)>,
) {
    let cx = *col_center_x.get(&signed_depth).unwrap_or(&MARGIN);
    let cy = top_y + rows / 2;
    let width = label_box_width(&node.label, node.shape);
    let node_ptr = node as *const MindNode;

    placed.insert(node_ptr, Placed { cx, cy, width });
    edges.push((parent_ptr, node_ptr));

    if node.children.is_empty() {
        return;
    }

    let _ = going_right;
    let mut child_top = top_y;
    for child in &node.children {
        let child_rows = subtree_rows(child);
        let next_depth = if going_right {
            signed_depth + 1
        } else {
            signed_depth - 1
        };
        layout_subtree(
            child,
            node_ptr,
            next_depth,
            going_right,
            child_top,
            child_rows,
            col_center_x,
            placed,
            edges,
        );
        child_top += child_rows + SIBLING_GAP;
    }
}

fn draw_tree(root: &MindNode, theme: &Theme) -> Canvas {
    let layout = layout_tree(root);
    let mut canvas = Canvas::new(layout.canvas_width, layout.canvas_height);

    let border_fg = Some(theme.code_border);
    let text_fg = Some(theme.fg);

    for (ptr, placed) in &layout.placed {
        let node = unsafe { &**ptr };
        let top_y = placed.cy.saturating_sub(NODE_HEIGHT / 2);
        canvas.draw_node(
            placed.cx,
            top_y,
            placed.width,
            &node.label,
            node.shape,
            border_fg,
            text_fg,
        );
    }

    for (edge_idx, (parent_ptr, child_ptr)) in layout.edges.iter().enumerate() {
        let parent = layout.placed.get(parent_ptr).copied();
        let child = layout.placed.get(child_ptr).copied();
        if let (Some(p), Some(c)) = (parent, child) {
            let going_right = p.cx < c.cx;
            let (parent_outer_x, child_outer_x) = if going_right {
                (right_edge(p.cx, p.width), left_edge(c.cx, c.width))
            } else {
                (left_edge(p.cx, p.width), right_edge(c.cx, c.width))
            };
            let fg = Some(edge_color(theme, edge_idx));
            canvas.draw_tree_edge(parent_outer_x, p.cy, child_outer_x, c.cy, fg);
        }
    }

    canvas
}

fn left_edge(cx: usize, width: usize) -> usize {
    cx.saturating_sub(width / 2)
}

fn right_edge(cx: usize, width: usize) -> usize {
    left_edge(cx, width) + width.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn render_rows(code: &str) -> Option<(Vec<Vec<crate::style::StyledSpan>>, usize)> {
        render(code, &Theme::dark())
    }

    fn row_text(row: &[crate::style::StyledSpan]) -> String {
        row.iter().map(|s| s.text.as_str()).collect()
    }

    // ───── Parser tests ─────

    #[test]
    fn parser_reads_simple_root_and_children() {
        let root = parse_mindmap("mindmap\n  root((R))\n    A\n    B").expect("parse");
        assert_eq!(root.label, "R");
        assert_eq!(root.shape, NodeShape::Circle);
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].label, "A");
        assert_eq!(root.children[1].label, "B");
    }

    #[test]
    fn parser_handles_indent_with_tabs_like_spaces() {
        let spaced = parse_mindmap("mindmap\n  root\n    A\n      A1").expect("spaced");
        let tabbed = parse_mindmap("mindmap\n\troot\n\t\tA\n\t\t\tA1").expect("tabbed");
        assert_eq!(spaced.children.len(), tabbed.children.len());
        assert_eq!(spaced.children[0].label, tabbed.children[0].label);
        assert_eq!(
            spaced.children[0].children.len(),
            tabbed.children[0].children.len()
        );
        assert_eq!(
            spaced.children[0].children[0].label,
            tabbed.children[0].children[0].label
        );
    }

    #[test]
    fn parser_shape_syntax_for_each_form() {
        let root =
            parse_mindmap("mindmap\n  root((Root))\n    r[Rect]\n    rd(Round)\n    d{Diamond}")
                .expect("parse");

        assert_eq!(root.shape, NodeShape::Circle);
        assert_eq!(root.children[0].label, "Rect");
        assert_eq!(root.children[0].shape, NodeShape::Rectangle);
        assert_eq!(root.children[1].label, "Round");
        assert_eq!(root.children[1].shape, NodeShape::Rounded);
        assert_eq!(root.children[2].label, "Diamond");
        assert_eq!(root.children[2].shape, NodeShape::Diamond);
    }

    #[test]
    fn parser_skips_classdef_and_strips_style_class_suffix() {
        let code =
            "mindmap\n  root\n    A\n  classDef foo fill:#fff\n    B:::foo\n  style A fill:#f00";
        let root = parse_mindmap(code).expect("parse");
        assert_eq!(root.label, "root");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].label, "A");
        assert_eq!(root.children[1].label, "B");
    }

    #[test]
    fn parser_rejects_missing_header() {
        assert!(parse_mindmap("graph TD\n  A --> B").is_none());
        assert!(parse_mindmap("mindmap").is_none());
    }

    // ───── Layout tests ─────

    #[test]
    fn subtree_units_leaf_is_one() {
        let leaf = MindNode {
            label: "x".into(),
            shape: NodeShape::Rectangle,
            children: Vec::new(),
        };
        assert_eq!(subtree_units(&leaf), 1);
    }

    #[test]
    fn subtree_units_sums_children() {
        let root = MindNode {
            label: "r".into(),
            shape: NodeShape::Circle,
            children: vec![
                MindNode {
                    label: "a".into(),
                    shape: NodeShape::Rectangle,
                    children: vec![
                        MindNode {
                            label: "a1".into(),
                            shape: NodeShape::Rectangle,
                            children: Vec::new(),
                        },
                        MindNode {
                            label: "a2".into(),
                            shape: NodeShape::Rectangle,
                            children: Vec::new(),
                        },
                    ],
                },
                MindNode {
                    label: "b".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
            ],
        };
        assert_eq!(subtree_units(&root), 3);
    }

    #[test]
    fn layout_partitions_even_right_odd_left() {
        let root = MindNode {
            label: "r".into(),
            shape: NodeShape::Circle,
            children: vec![
                MindNode {
                    label: "even0".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
                MindNode {
                    label: "odd1".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
                MindNode {
                    label: "even2".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
            ],
        };

        let layout = layout_tree(&root);
        let root_placed = layout.placed[&layout.root_ptr];

        let mut right_count = 0;
        let mut left_count = 0;
        for (ptr, placed) in &layout.placed {
            if *ptr == layout.root_ptr {
                continue;
            }
            if placed.cx > root_placed.cx {
                right_count += 1;
            } else if placed.cx < root_placed.cx {
                left_count += 1;
            }
        }
        assert_eq!(right_count, 2, "even-index (0,2) -> right");
        assert_eq!(left_count, 1, "odd-index (1) -> left");
    }

    #[test]
    fn layout_root_is_vertically_centered_against_children() {
        let root = MindNode {
            label: "r".into(),
            shape: NodeShape::Circle,
            children: vec![
                MindNode {
                    label: "a".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
                MindNode {
                    label: "b".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
                MindNode {
                    label: "c".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
                MindNode {
                    label: "d".into(),
                    shape: NodeShape::Rectangle,
                    children: Vec::new(),
                },
            ],
        };

        let layout = layout_tree(&root);
        let root_placed = layout.placed[&layout.root_ptr];

        let child_ys: Vec<usize> = layout
            .placed
            .iter()
            .filter(|(ptr, _)| **ptr != layout.root_ptr)
            .map(|(_, p)| p.cy)
            .collect();
        let min_y = *child_ys.iter().min().unwrap();
        let max_y = *child_ys.iter().max().unwrap();
        let midpoint = (min_y + max_y) / 2;
        assert!(
            root_placed.cy == midpoint || root_placed.cy + 1 == midpoint,
            "root cy {} should be near midpoint {}",
            root_placed.cy,
            midpoint
        );
    }

    // ───── Renderer smoke tests ─────

    #[test]
    fn render_smoke_includes_root_and_level1_labels() {
        let (rows, _w) =
            render_rows("mindmap\n  root((The Root))\n    Alpha\n    Beta").expect("rendered");
        let combined: String = rows
            .iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            combined.contains("The Root"),
            "root label missing:\n{combined}"
        );
        assert!(combined.contains("Alpha"), "Alpha missing:\n{combined}");
        assert!(combined.contains("Beta"), "Beta missing:\n{combined}");
    }

    #[test]
    fn render_smoke_root_circle_glyph_for_double_parens() {
        let (rows, _w) = render_rows("mindmap\n  root((Center))\n    A").expect("rendered");
        let combined: String = rows
            .iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            combined.contains('╭') && combined.contains('╮'),
            "expected rounded (circle) corners around root:\n{combined}"
        );
    }

    #[test]
    fn render_smoke_emits_tree_edge_glyphs() {
        let (rows, _w) =
            render_rows("mindmap\n  root\n    A\n    B\n    C\n    D").expect("rendered");
        let combined: String = rows
            .iter()
            .map(|r| row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        let has_corner = combined.contains('╮')
            || combined.contains('╯')
            || combined.contains('╭')
            || combined.contains('╰');
        let has_horizontal = combined.contains('─');
        assert!(has_corner, "expected a rounded corner glyph:\n{combined}");
        assert!(
            has_horizontal,
            "expected horizontal connector ─:\n{combined}"
        );
    }

    #[test]
    fn render_layout_regression_positive_dimensions_and_root_position() {
        let (rows, w) = render_rows("mindmap\n  root((R))\n    A\n      A1\n      A2\n    B")
            .expect("rendered");
        assert!(w > 0, "canvas width should be positive, got {w}");
        assert!(
            !rows.is_empty(),
            "should produce at least one row of output"
        );
        let max_row_len = rows
            .iter()
            .map(|r| r.iter().map(|s| s.text.chars().count()).sum::<usize>())
            .max()
            .unwrap_or(0);
        assert!(
            max_row_len <= w,
            "row length {max_row_len} should fit within canvas width {w}"
        );

        let root_row_idx = rows
            .iter()
            .position(|r| row_text(r).contains("R"))
            .expect("root label appears in output");
        let _ = root_row_idx;
    }
}
