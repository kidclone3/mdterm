use crossterm::style::Color;

use crate::style::StyledSpan;
use crate::theme::Theme;

use super::canvas::{CONN_LEFT, CONN_RIGHT, Canvas, NodeShape};
use super::theme::edge_color;

// ───── Sequence diagrams ─────

const SEQ_SELF_W: usize = 4;

#[derive(Clone, Copy, PartialEq, Debug)]
enum SeqHead {
    Solid, // ->>
    Open,  // -)
    Cross, // -x
    None,  // -> (plain line, no arrowhead)
}

struct SeqMessage {
    from: usize,
    to: usize,
    label: String,
    dashed: bool,
    head: SeqHead,
}

#[derive(Clone, Copy, PartialEq)]
enum NotePlacement {
    Over,
    LeftOf,
    RightOf,
}

enum SeqEvent {
    Message(SeqMessage),
    Note {
        lo: usize,
        hi: usize,
        placement: NotePlacement,
        text: String,
    },
    Block(String),
}

struct SeqParticipant {
    id: String,
    label: String,
}

/// Strip a single pair of surrounding double quotes, as used by
/// `participant A as "Display Name"`.
fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(s)
}

/// Find a participant by id, or create one (preserving declaration order).
fn seq_intern(parts: &mut Vec<SeqParticipant>, id: &str, label: Option<&str>) -> usize {
    if let Some(idx) = parts.iter().position(|p| p.id == id) {
        if let Some(l) = label {
            parts[idx].label = l.to_string();
        }
        idx
    } else {
        parts.push(SeqParticipant {
            id: id.to_string(),
            label: label.unwrap_or(id).to_string(),
        });
        parts.len() - 1
    }
}

/// Locate the message arrow operator in a line, returning
/// `(start, end, dashed, head)`.
///
/// Arrows are matched in two priority tiers, each scanned left-to-right with
/// longest-pattern-first at every position. Tier 1 holds the unambiguous
/// `>`-terminated forms; tier 2 holds `-x`/`-)`, whose two-char shapes could
/// otherwise match inside a hyphenated participant id (e.g. the `-x` inside
/// `multi-xenon->>Bob`). Checking tier 1 across the whole line first means a
/// real `->>` always wins over an incidental `-x`.
fn detect_seq_arrow(s: &str) -> Option<(usize, usize, bool, SeqHead)> {
    const TIER1: &[(&str, bool, SeqHead)] = &[
        ("-->>", true, SeqHead::Solid),
        ("->>", false, SeqHead::Solid),
        ("-->", true, SeqHead::None),
        ("->", false, SeqHead::None),
    ];
    const TIER2: &[(&str, bool, SeqHead)] = &[
        ("--)", true, SeqHead::Open),
        ("-)", false, SeqHead::Open),
        ("--x", true, SeqHead::Cross),
        ("-x", false, SeqHead::Cross),
    ];
    for tier in [TIER1, TIER2] {
        for (i, _) in s.char_indices() {
            for &(pat, dashed, head) in tier {
                if s[i..].starts_with(pat) {
                    return Some((i, i + pat.len(), dashed, head));
                }
            }
        }
    }
    None
}

fn parse_sequence(code: &str) -> Option<(Vec<SeqParticipant>, Vec<SeqEvent>)> {
    let mut parts: Vec<SeqParticipant> = Vec::new();
    let mut events: Vec<SeqEvent> = Vec::new();

    for raw in code.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let first = line.split_whitespace().next().unwrap_or("");
        if first == "sequenceDiagram" {
            continue;
        }

        // Participant / actor declarations: `participant Bob as Bob B.`
        if first == "participant" || first == "actor" {
            let rest = line[first.len()..].trim();
            if let Some((id, label)) = rest.split_once(" as ") {
                seq_intern(&mut parts, id.trim(), Some(unquote(label.trim())));
            } else if !rest.is_empty() {
                seq_intern(&mut parts, rest, Some(unquote(rest)));
            }
            continue;
        }

        // Notes
        if first == "Note" || first == "note" {
            if let Some(ev) = parse_seq_note(line, &mut parts) {
                events.push(ev);
            }
            continue;
        }

        // Block-structure keywords
        match first {
            "loop" | "alt" | "opt" | "else" | "par" | "and" | "critical" | "break" => {
                events.push(SeqEvent::Block(line.to_string()));
                continue;
            }
            "end" | "activate" | "deactivate" | "autonumber" | "box" | "rect" | "link"
            | "links" | "title" | "create" | "destroy" => {
                continue;
            }
            _ => {}
        }

        // Messages
        if let Some(ev) = parse_seq_message(line, &mut parts) {
            events.push(ev);
        }
    }

    if parts.is_empty() {
        return None;
    }
    Some((parts, events))
}

fn parse_seq_message(line: &str, parts: &mut Vec<SeqParticipant>) -> Option<SeqEvent> {
    let (start, end, dashed, head) = detect_seq_arrow(line)?;
    let from_s = line[..start].trim();
    let (to_s, label) = match line[end..].split_once(':') {
        Some((a, b)) => (a.trim(), b.trim().to_string()),
        None => (line[end..].trim(), String::new()),
    };
    // Strip activation markers (`Alice->>+Bob`, `Bob-->>-Alice`).
    let from_id = from_s.trim_start_matches(['+', '-']).trim();
    let to_id = to_s.trim_start_matches(['+', '-']).trim();
    if from_id.is_empty() || to_id.is_empty() {
        return None;
    }
    let from = seq_intern(parts, from_id, None);
    let to = seq_intern(parts, to_id, None);
    Some(SeqEvent::Message(SeqMessage {
        from,
        to,
        label,
        dashed,
        head,
    }))
}

fn parse_seq_note(line: &str, parts: &mut Vec<SeqParticipant>) -> Option<SeqEvent> {
    // `line` begins with "Note"/"note" (both 4 bytes).
    let rest = line[4..].trim();
    let (spec, text) = match rest.split_once(':') {
        Some((a, b)) => (a.trim(), b.trim().to_string()),
        None => (rest, String::new()),
    };
    let (placement, targets) = if let Some(r) = spec.strip_prefix("right of ") {
        (NotePlacement::RightOf, r)
    } else if let Some(r) = spec.strip_prefix("left of ") {
        (NotePlacement::LeftOf, r)
    } else if let Some(r) = spec.strip_prefix("over ") {
        (NotePlacement::Over, r)
    } else {
        return None;
    };
    let mut idxs: Vec<usize> = targets
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| seq_intern(parts, t, None))
        .collect();
    if idxs.is_empty() {
        return None;
    }
    idxs.sort_unstable();
    Some(SeqEvent::Note {
        lo: *idxs.first().unwrap(),
        hi: *idxs.last().unwrap(),
        placement,
        text,
    })
}

/// Widen the center-to-center gaps spanning columns `lo..hi` so their total is
/// at least `need`, distributing any deficit evenly.
fn seq_ensure_span(gaps: &mut [usize], lo: usize, hi: usize, need: usize) {
    if lo >= hi {
        return;
    }
    let span: usize = gaps[lo..hi].iter().sum();
    if need > span {
        let deficit = need - span;
        let k = hi - lo;
        let per = deficit / k;
        let rem = deficit % k;
        for (off, g) in gaps[lo..hi].iter_mut().enumerate() {
            *g += per + usize::from(off < rem);
        }
    }
}

fn draw_self_message(
    canvas: &mut Canvas,
    cx: usize,
    y0: usize,
    label: &str,
    dashed: bool,
    head: SeqHead,
    color: Option<Color>,
) {
    let right_x = cx + SEQ_SELF_W;
    let line = if dashed { '┄' } else { '─' };
    let (r1, r2, r3) = (y0 + 1, y0 + 2, y0 + 3);

    // Top of the loop, leaving the lifeline rightwards.
    canvas.add_connection(cx, r1, CONN_RIGHT, color);
    for x in (cx + 1)..right_x {
        if dashed {
            canvas.set(x, r1, line, color);
        } else {
            canvas.set_edge(x, r1, '─', color);
        }
    }
    canvas.set(right_x, r1, '╮', color);
    canvas.set(right_x, r2, '│', color);
    canvas.set(right_x, r3, '╯', color);
    for x in (cx + 1)..right_x {
        if dashed {
            canvas.set(x, r3, line, color);
        } else {
            canvas.set_edge(x, r3, '─', color);
        }
    }

    // Arrowhead returning into the lifeline.
    let head_ch = match head {
        SeqHead::Solid => '◀',
        SeqHead::Open => '◁',
        SeqHead::Cross => '×',
        SeqHead::None => '─',
    };
    canvas.set(cx, r3, head_ch, color);

    for (i, ch) in label.chars().enumerate() {
        canvas.set(right_x + 2 + i, y0, ch, color);
    }
}

fn render_sequence(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    let (parts, events) = parse_sequence(code)?;
    let n = parts.len();

    let border_fg = Some(theme.code_border);
    let text_fg = Some(theme.fg);
    let char_len = |s: &str| s.chars().count();

    let box_w: Vec<usize> = parts
        .iter()
        .map(|p| (char_len(&p.label) + 4).max(7))
        .collect();

    // Center-to-center distance between adjacent lifelines.
    let mut gaps: Vec<usize> = (0..n.saturating_sub(1))
        .map(|i| box_w[i] / 2 + box_w[i + 1] / 2 + 6)
        .collect();
    let mut left_margin = 2usize;
    let mut right_margin = 2usize;
    let mut min_width = 0usize; // floor driven by centered block labels

    // Reserve horizontal room so message and note labels fit their spans.
    for ev in &events {
        match ev {
            SeqEvent::Message(m) => {
                if m.from == m.to {
                    let need = SEQ_SELF_W + 2 + char_len(&m.label);
                    let i = m.from;
                    if i + 1 < n {
                        gaps[i] = gaps[i].max(box_w[i + 1] / 2 + need + 2);
                    } else {
                        right_margin = right_margin.max(need + 2);
                    }
                } else {
                    let (lo, hi) = (m.from.min(m.to), m.from.max(m.to));
                    seq_ensure_span(&mut gaps, lo, hi, char_len(&m.label) + 4);
                }
            }
            SeqEvent::Note {
                lo,
                hi,
                placement,
                text,
            } => {
                let w = char_len(text) + 4;
                match placement {
                    NotePlacement::Over if lo == hi => {
                        let half = w / 2 + 1;
                        let i = *lo;
                        if i > 0 {
                            gaps[i - 1] = gaps[i - 1].max(box_w[i - 1] / 2 + half);
                        } else {
                            left_margin = left_margin.max(half + 1);
                        }
                        if i + 1 < n {
                            gaps[i] = gaps[i].max(box_w[i + 1] / 2 + half);
                        } else {
                            right_margin = right_margin.max(half + 1);
                        }
                    }
                    NotePlacement::Over => seq_ensure_span(&mut gaps, *lo, *hi, w),
                    NotePlacement::RightOf => {
                        let i = *hi;
                        if i + 1 < n {
                            gaps[i] = gaps[i].max(box_w[i + 1] / 2 + w + 2);
                        } else {
                            right_margin = right_margin.max(w + 2);
                        }
                    }
                    NotePlacement::LeftOf => {
                        let i = *lo;
                        if i > 0 {
                            gaps[i - 1] = gaps[i - 1].max(box_w[i - 1] / 2 + w + 2);
                        } else {
                            left_margin = left_margin.max(w + 2);
                        }
                    }
                }
            }
            SeqEvent::Block(t) => min_width = min_width.max(char_len(t) + 4),
        }
    }

    let mut centers: Vec<usize> = Vec::with_capacity(n);
    centers.push(left_margin + box_w[0] / 2);
    for i in 1..n {
        centers.push(centers[i - 1] + gaps[i - 1]);
    }
    let canvas_width = (centers[n - 1] + box_w[n - 1] / 2 + right_margin + 1).max(min_width);

    // Vertical layout: header boxes, then one stacked row-block per event.
    let header_h = 3usize;
    let mut event_y: Vec<usize> = Vec::with_capacity(events.len());
    let mut y = header_h + 1;
    for ev in &events {
        event_y.push(y);
        y += match ev {
            SeqEvent::Message(m) if m.from == m.to => 4,
            SeqEvent::Message(_) => 2,
            SeqEvent::Note { .. } => 4,
            SeqEvent::Block(_) => 2,
        };
    }
    let canvas_height = y + 1;

    let mut canvas = Canvas::new(canvas_width, canvas_height);

    // Participant boxes.
    for i in 0..n {
        canvas.draw_node(
            centers[i],
            0,
            box_w[i],
            &parts[i].label,
            NodeShape::Rounded,
            border_fg,
            text_fg,
        );
    }

    // Lifelines (drawn with set_edge so message crossings form junctions).
    for &cx in &centers {
        for ly in header_h..(canvas_height - 1) {
            canvas.set_edge(cx, ly, '│', border_fg);
        }
    }

    for (idx, ev) in events.iter().enumerate() {
        let y0 = event_y[idx];
        match ev {
            SeqEvent::Message(m) => {
                let color = Some(edge_color(theme, idx));
                if m.from == m.to {
                    draw_self_message(
                        &mut canvas,
                        centers[m.from],
                        y0,
                        &m.label,
                        m.dashed,
                        m.head,
                        color,
                    );
                    continue;
                }
                let (sx, dx) = (centers[m.from], centers[m.to]);
                let rightward = dx >= sx;
                let (lo, hi) = (sx.min(dx), sx.max(dx));
                let line_y = y0 + 1;

                // Mid-segment between the two lifelines.
                for x in (lo + 1)..hi {
                    if m.dashed {
                        canvas.set(x, line_y, '┄', color);
                    } else {
                        canvas.set_edge(x, line_y, '─', color);
                    }
                }

                // Source endpoint: tee off the lifeline (├ / ┤).
                let (src_x, dst_x) = if rightward { (lo, hi) } else { (hi, lo) };
                let leave = if rightward { CONN_RIGHT } else { CONN_LEFT };
                canvas.add_connection(src_x, line_y, leave, color);

                // Destination endpoint: arrowhead, or tee for headless lines.
                let head_ch = match (m.head, rightward) {
                    (SeqHead::Solid, true) => Some('▶'),
                    (SeqHead::Solid, false) => Some('◀'),
                    (SeqHead::Open, true) => Some('▷'),
                    (SeqHead::Open, false) => Some('◁'),
                    (SeqHead::Cross, _) => Some('×'),
                    (SeqHead::None, _) => Option::None,
                };
                match head_ch {
                    Some(ch) => canvas.set(dst_x, line_y, ch, color),
                    None => canvas.add_connection(
                        dst_x,
                        line_y,
                        if rightward { CONN_LEFT } else { CONN_RIGHT },
                        color,
                    ),
                }
                if !m.label.is_empty() {
                    let mid = (lo + hi) / 2;
                    let start = mid.saturating_sub(char_len(&m.label) / 2);
                    for (i, ch) in m.label.chars().enumerate() {
                        canvas.set(start + i, y0, ch, color);
                    }
                }
            }
            SeqEvent::Note {
                lo,
                hi,
                placement,
                text,
            } => {
                let w = char_len(text) + 4;
                let (cx, bw) = match placement {
                    NotePlacement::Over if lo == hi => (centers[*lo], w),
                    NotePlacement::Over => {
                        let cx = (centers[*lo] + centers[*hi]) / 2;
                        let span = centers[*hi] - centers[*lo] + box_w[*lo] / 2 + box_w[*hi] / 2;
                        (cx, span.max(w))
                    }
                    NotePlacement::RightOf => (centers[*hi] + w / 2 + 2, w),
                    NotePlacement::LeftOf => (centers[*lo].saturating_sub(w / 2 + 2), w),
                };
                canvas.draw_node_with_height(
                    cx,
                    y0,
                    bw,
                    3,
                    text,
                    NodeShape::Rounded,
                    border_fg,
                    text_fg,
                );
            }
            SeqEvent::Block(t) => {
                let start = canvas_width.saturating_sub(char_len(t)) / 2;
                for (i, ch) in t.chars().enumerate() {
                    canvas.set(start + i, y0, ch, border_fg);
                }
            }
        }
    }

    let rows = canvas.to_span_rows(theme);
    Some((rows, canvas_width))
}

// ───── Public entry for the dispatcher ─────

pub(crate) fn render(code: &str, theme: &Theme) -> Option<(Vec<Vec<StyledSpan>>, usize)> {
    render_sequence(code, theme)
}

#[cfg(test)]
mod tests {
    use super::super::render_mermaid;
    use super::*;
    use crate::theme::Theme;

    fn seq_text(input: &str) -> String {
        let theme = Theme::dark();
        let (rows, width) = render_mermaid(input, &theme).expect("sequence diagram should render");
        assert!(width > 0, "rendered diagram should have positive width");
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|span| span.text.as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn sequence_diagram_renders_participants_and_arrow() {
        let out = seq_text(
            "sequenceDiagram\n    participant Alice\n    participant Bob\n    Alice->>Bob: Hello\n    Bob-->>Alice: Hi there",
        );
        assert!(out.contains("Alice"), "should show participant Alice");
        assert!(out.contains("Bob"), "should show participant Bob");
        assert!(out.contains("Hello"), "should show the message label");
        assert!(
            out.contains('▶') || out.contains('◀'),
            "should draw a message arrowhead"
        );
    }

    #[test]
    fn sequence_diagram_not_parsed_as_flowchart() {
        // Regression: the flowchart parser used to turn `participant`/the
        // diagram header into phantom nodes.
        let out = seq_text("sequenceDiagram\n    participant Alice\n    Alice->>Bob: hi");
        assert!(
            !out.contains("participant"),
            "must not render the 'participant' keyword as a node"
        );
        assert!(
            !out.contains("sequenceDiagram"),
            "must not render the diagram header as a node"
        );
    }

    #[test]
    fn sequence_diagram_supports_notes_and_aliases() {
        let out = seq_text(
            "sequenceDiagram\n    participant A as Alice\n    A->>A: think\n    Note over A: pondering",
        );
        assert!(out.contains("Alice"), "alias label should be used");
        assert!(out.contains("pondering"), "note text should render");
    }

    #[test]
    fn detect_seq_arrow_prefers_longest_match() {
        assert_eq!(
            detect_seq_arrow("A-->>B"),
            Some((1, 5, true, SeqHead::Solid))
        );
        assert_eq!(
            detect_seq_arrow("A->>B"),
            Some((1, 4, false, SeqHead::Solid))
        );
        assert_eq!(detect_seq_arrow("A-->B"), Some((1, 4, true, SeqHead::None)));
        assert_eq!(detect_seq_arrow("A-)B"), Some((1, 3, false, SeqHead::Open)));
    }

    #[test]
    fn detect_seq_arrow_ignores_hyphen_inside_id() {
        // The `-x` inside `multi-xenon` must not win over the real `->>`.
        assert_eq!(
            detect_seq_arrow("multi-xenon->>Bob"),
            Some((11, 14, false, SeqHead::Solid))
        );
    }

    #[test]
    fn sequence_diagram_strips_alias_quotes() {
        let out = seq_text("sequenceDiagram\n    participant A as \"Alice Smith\"\n    A->>A: hi");
        assert!(out.contains("Alice Smith"), "alias text should render");
        assert!(!out.contains('"'), "surrounding quotes should be stripped");
    }

    #[test]
    fn sequence_block_label_not_truncated() {
        let label = "loop while the queue is not empty and retries remain";
        let out = seq_text(&format!(
            "sequenceDiagram\n    participant A\n    participant B\n    {label}\n    A->>B: x\n    end"
        ));
        assert!(
            out.contains(label),
            "block label must be reserved width, not truncated"
        );
    }
}
