use crate::style::{Style, StyledSpan};
use crate::theme::Theme;
use crossterm::style::Color;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NodeShape {
    Rectangle,
    Rounded,
    Diamond,
    Circle,
    Final,
    ForkBar,
}

/// A row to be drawn inside a multi-row card node.
pub(crate) struct CardDrawRow {
    pub key: String,
    pub value_text: String,
    pub value_color: Option<Color>,
    /// Per-row override for the key column color. When `None`, the card-level
    /// `key_fg` is used; when `Some`, only this row's key text adopts the color.
    pub key_color: Option<Color>,
    /// If true, the value area shows `──▶` instead of text.
    pub is_connector: bool,
}

/// Endpoint decoration for one side of a rendered edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeEnd {
    None,
    Arrow,
    HollowArrow,
    FilledDiamond,
    HollowDiamond,
}

/// Crow's-foot cardinality, used by erDiagram edges. Encoded separately from
/// `EdgeEnd` because a crow's-foot is a 2-character decoration rather than a
/// single arrowhead glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Card {
    One,
    ZeroOrOne,
    ZeroOrMany,
    OneOrMany,
}

/// Which side of a top-down edge a crow's-foot decoration sits on.
/// `Down` = the decoration hangs below an entity (source end of a TD edge);
/// `Up` = it rises above an entity (destination end of a TD edge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrowDir {
    Up,
    Down,
}

/// Style parameters shared by `draw_edge_td` / `draw_edge_lr`. The label is
/// borrowed from the caller for the duration of the draw call; all other
/// fields are by-value.
pub(crate) struct EdgeStyle<'a> {
    pub dashed: bool,
    pub head: EdgeEnd,
    pub tail: EdgeEnd,
    pub label: Option<&'a str>,
    pub far_label: Option<&'a str>,
}

impl<'a> Default for EdgeStyle<'a> {
    fn default() -> Self {
        Self {
            dashed: false,
            head: EdgeEnd::None,
            tail: EdgeEnd::None,
            label: None,
            far_label: None,
        }
    }
}

/// Map an `EdgeEnd` decoration to its rendered glyph for a top-down edge.
/// `is_head = true` selects the destination-side glyph (pointing down, into
/// the destination); `is_head = false` selects the mirrored source-side glyph
/// (pointing up, into the source). Diamonds render identically at either end.
fn edge_end_glyph_td(end: EdgeEnd, is_head: bool) -> Option<char> {
    match end {
        EdgeEnd::None => None,
        EdgeEnd::Arrow if is_head => Some('▼'),
        EdgeEnd::Arrow => Some('▲'),
        EdgeEnd::HollowArrow if is_head => Some('▽'),
        EdgeEnd::HollowArrow => Some('△'),
        EdgeEnd::FilledDiamond => Some('◆'),
        EdgeEnd::HollowDiamond => Some('◇'),
    }
}

/// Map an `EdgeEnd` decoration to its rendered glyph for a left-right edge.
/// `is_head = true` selects the destination-side glyph (pointing right, into
/// the destination); `is_head = false` selects the mirrored source-side glyph
/// (pointing left, into the source). Diamonds render identically at either end.
fn edge_end_glyph_lr(end: EdgeEnd, is_head: bool) -> Option<char> {
    match end {
        EdgeEnd::None => None,
        EdgeEnd::Arrow if is_head => Some('▶'),
        EdgeEnd::Arrow => Some('◀'),
        EdgeEnd::HollowArrow if is_head => Some('▷'),
        EdgeEnd::HollowArrow => Some('◁'),
        EdgeEnd::FilledDiamond => Some('◆'),
        EdgeEnd::HollowDiamond => Some('◇'),
    }
}

/// Resolve the two characters that make up a crow's-foot endpoint decoration.
/// Returns `(near_entity_char, near_trunk_char)` — the character closest to the
/// entity box and the character closest to the connecting edge trunk. The
/// `Down` variant mirrors the left-side tokens of the spec table; `Up` mirrors
/// the right-side (mirrored) tokens. The fork glyphs `⟨`/`⟩` indicate "many".
fn crowsfoot_chars(card: Card, dir: CrowDir) -> (char, char) {
    match (card, dir) {
        (Card::One, CrowDir::Down) => ('│', '|'),
        (Card::ZeroOrOne, CrowDir::Down) => ('│', 'o'),
        (Card::ZeroOrMany, CrowDir::Down) => ('⟨', 'o'),
        (Card::OneOrMany, CrowDir::Down) => ('⟨', '|'),
        (Card::One, CrowDir::Up) => ('│', '|'),
        (Card::ZeroOrOne, CrowDir::Up) => ('│', 'o'),
        (Card::ZeroOrMany, CrowDir::Up) => ('⟩', 'o'),
        (Card::OneOrMany, CrowDir::Up) => ('⟩', '|'),
    }
}

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
        // ForkBar: single-row `═` bar laid across the middle of the 3-row slot.
        if shape == NodeShape::ForkBar {
            let x = cx.saturating_sub(width / 2);
            for i in 0..width {
                self.set_node(x + i, y + 1, '═', border_fg);
            }
            return;
        }

        let x = cx.saturating_sub(width / 2);

        let (tl, tr, bl, br, h, v) = match shape {
            NodeShape::Rectangle => ('┌', '┐', '└', '┘', '─', '│'),
            NodeShape::Rounded | NodeShape::Circle | NodeShape::Final => {
                ('╭', '╮', '╰', '╯', '─', '│')
            }
            NodeShape::Diamond => ('◆', '◆', '◆', '◆', '─', '│'),
            NodeShape::ForkBar => unreachable!(),
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
        // ForkBar: a single-row `═` bar centered vertically in the slot.
        if shape == NodeShape::ForkBar {
            let height = height.max(3);
            let x = cx.saturating_sub(width / 2);
            let bar_y = y + height / 2;
            for i in 0..width {
                self.set_node(x + i, bar_y, '═', border_fg);
            }
            return;
        }

        let height = height.max(3);
        let x = cx.saturating_sub(width / 2);

        let (tl, tr, bl, br, h, v) = match shape {
            NodeShape::Rectangle => ('┌', '┐', '└', '┘', '─', '│'),
            NodeShape::Rounded | NodeShape::Circle | NodeShape::Final => ('╭', '╮', '╰', '╯', '─', '│'),
            NodeShape::Diamond => ('◆', '◆', '◆', '◆', '─', '│'),
            NodeShape::ForkBar => unreachable!(),
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

    /// Draw the outer frame + tinted background for a composite (nested) state
    /// node. The inner sub-canvas is stamped separately via `stamp_canvas`.
    /// Title sits on the top border (matching `draw_card` styling).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_composite_outer(
        &mut self,
        left_x: usize,
        top_y: usize,
        width: usize,
        height: usize,
        title: &str,
        border_fg: Option<Color>,
        title_fg: Option<Color>,
        bg: Option<Color>,
    ) {
        if width < 4 || height < 3 {
            return;
        }

        // Top border with title: ╭─ Title ───╮
        self.set_node_bg(left_x, top_y, '╭', border_fg, bg);
        self.set_node_bg(left_x + 1, top_y, '─', border_fg, bg);
        let title_chars: Vec<char> = title.chars().collect();
        let max_title = width.saturating_sub(4);
        let show_title = title_chars.len().min(max_title);
        for (i, &ch) in title_chars[..show_title].iter().enumerate() {
            self.set_node_bg(left_x + 2 + i, top_y, ch, title_fg, bg);
        }
        for x in (left_x + 2 + show_title)..(left_x + width - 1) {
            self.set_node_bg(x, top_y, '─', border_fg, bg);
        }
        self.set_node_bg(left_x + width - 1, top_y, '╮', border_fg, bg);

        // Side borders + tinted background fill
        for row_y in (top_y + 1)..(top_y + height - 1) {
            self.set_node_bg(left_x, row_y, '│', border_fg, bg);
            for x in (left_x + 1)..(left_x + width - 1) {
                self.set_node_bg(x, row_y, ' ', title_fg, bg);
            }
            self.set_node_bg(left_x + width - 1, row_y, '│', border_fg, bg);
        }

        // Bottom border: ╰───╯
        let bot_y = top_y + height - 1;
        self.set_node_bg(left_x, bot_y, '╰', border_fg, bg);
        for x in (left_x + 1)..(left_x + width - 1) {
            self.set_node_bg(x, bot_y, '─', border_fg, bg);
        }
        self.set_node_bg(left_x + width - 1, bot_y, '╯', border_fg, bg);
    }

    /// Copy another canvas's non-empty cells into this one at offset (dx, dy).
    /// Used to embed a sub-canvas (composite state's inner graph) inside the
    /// parent canvas region already painted by `draw_composite_outer`.
    pub(crate) fn stamp_canvas(&mut self, other: &Canvas, dx: usize, dy: usize) {
        for y in 0..other.height {
            if dy + y >= self.height {
                break;
            }
            for x in 0..other.width {
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
            let row_key_fg = if is_highlight {
                highlight_fg
            } else {
                row.key_color.or(key_fg)
            };
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
        style: EdgeStyle,
        fg: Option<Color>,
    ) {
        if src_bottom_y + 1 >= dst_top_y {
            return;
        }

        let mid_y = src_bottom_y + 1 + (dst_top_y - src_bottom_y - 1) / 2;
        let vert_ch: char = if style.dashed { '┊' } else { '│' };
        let horz_ch: char = if style.dashed { '┄' } else { '─' };
        let head_ch: Option<char> = edge_end_glyph_td(style.head, true);
        let tail_ch: Option<char> = edge_end_glyph_td(style.tail, false);

        // Helper: lay down a vertical body cell, honoring dashed style. Solid
        // segments go through `add_connection` so junctions compose correctly
        // (flowchart/sequence regression); dashed segments use `set` because
        // dashed glyphs do not participate in junction merging.
        let draw_vert = |canvas: &mut Self, x: usize, y: usize| {
            if style.dashed {
                canvas.set(x, y, vert_ch, fg);
            } else {
                canvas.add_connection(x, y, CONN_UP | CONN_DOWN, fg);
            }
        };
        let draw_horz = |canvas: &mut Self, x: usize, y: usize| {
            if style.dashed {
                canvas.set(x, y, horz_ch, fg);
            } else {
                canvas.add_connection(x, y, CONN_LEFT | CONN_RIGHT, fg);
            }
        };

        if src_cx == dst_cx {
            // Straight down. Reserve the first row for a tail glyph and the
            // last row for a head glyph; fill everything in between with body.
            let y_first = src_bottom_y + 1;
            let y_last = dst_top_y - 1;
            let body_lo = if tail_ch.is_some() { y_first + 1 } else { y_first };
            let body_hi = if head_ch.is_some() { y_last } else { dst_top_y };
            for y in body_lo..body_hi {
                draw_vert(self, src_cx, y);
            }
            if let Some(tc) = tail_ch {
                self.set(src_cx, y_first, tc, fg);
            }
            if let Some(hc) = head_ch {
                self.set(dst_cx, y_last, hc, fg);
            }

            // Place label beside the vertical line
            if let Some(text) = style.label {
                let label_y = src_bottom_y + 1;
                for (i, ch) in text.chars().enumerate() {
                    self.set(src_cx + 2 + i, label_y, ch, fg);
                }
            }
            // Far label (cardinality / source-end text) sits one row below.
            if let Some(text) = style.far_label {
                let label_y = src_bottom_y + 2;
                if label_y < dst_top_y {
                    for (i, ch) in text.chars().enumerate() {
                        self.set(src_cx + 2 + i, label_y, ch, fg);
                    }
                }
            }
        } else {
            // Down from source to mid_y
            let src_body_lo = if tail_ch.is_some() {
                src_bottom_y + 2
            } else {
                src_bottom_y + 1
            };
            for y in src_body_lo..mid_y {
                draw_vert(self, src_cx, y);
            }
            if let Some(tc) = tail_ch {
                self.set(src_cx, src_bottom_y + 1, tc, fg);
            }

            // Junction at source column, mid_y
            let src_turn = if dst_cx > src_cx {
                CONN_UP | CONN_RIGHT
            } else {
                CONN_UP | CONN_LEFT
            };
            if style.dashed {
                self.set(src_cx, mid_y, junction_char(src_turn), fg);
            } else {
                self.add_connection(src_cx, mid_y, src_turn, fg);
            }

            // Horizontal segment
            let (min_x, max_x) = if src_cx < dst_cx {
                (src_cx, dst_cx)
            } else {
                (dst_cx, src_cx)
            };
            for x in (min_x + 1)..max_x {
                draw_horz(self, x, mid_y);
            }

            // Junction at destination column, mid_y
            let dst_turn = if dst_cx > src_cx {
                CONN_LEFT | CONN_DOWN
            } else {
                CONN_RIGHT | CONN_DOWN
            };
            if style.dashed {
                self.set(dst_cx, mid_y, junction_char(dst_turn), fg);
            } else {
                self.add_connection(dst_cx, mid_y, dst_turn, fg);
            }

            // Down from mid_y to destination
            let dst_body_hi = if head_ch.is_some() { dst_top_y - 1 } else { dst_top_y };
            for y in (mid_y + 1)..dst_body_hi {
                draw_vert(self, dst_cx, y);
            }
            if let Some(hc) = head_ch {
                self.set(dst_cx, dst_top_y - 1, hc, fg);
            }

            // Place label above horizontal segment
            if let Some(text) = style.label {
                let label_len = text.chars().count();
                let label_start = min_x + (max_x - min_x).saturating_sub(label_len) / 2;
                let label_y = if mid_y > 0 { mid_y - 1 } else { mid_y };
                for (i, ch) in text.chars().enumerate() {
                    let lx = label_start + i;
                    if lx < self.width {
                        self.set(lx, label_y, ch, fg);
                    }
                }
            }
            // Far label near the source side of the horizontal segment.
            if let Some(text) = style.far_label {
                let label_y = if mid_y > 0 { mid_y - 1 } else { mid_y };
                let src_label_x = if dst_cx > src_cx {
                    src_cx + 2
                } else {
                    src_cx.saturating_sub(2 + text.chars().count())
                };
                for (i, ch) in text.chars().enumerate() {
                    let lx = src_label_x + i;
                    if lx < self.width {
                        self.set(lx, label_y, ch, fg);
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
        style: EdgeStyle,
        fg: Option<Color>,
        mid_x_override: Option<usize>,
    ) {
        if src_right_x + 1 >= dst_left_x {
            return;
        }

        let mid_x =
            mid_x_override.unwrap_or_else(|| src_right_x + 1 + (dst_left_x - src_right_x - 1) / 2);

        let horz_ch: char = if style.dashed { '┄' } else { '─' };
        let vert_ch: char = if style.dashed { '┊' } else { '│' };
        let head_ch: Option<char> = edge_end_glyph_lr(style.head, true);
        let tail_ch: Option<char> = edge_end_glyph_lr(style.tail, false);

        // Helper: lay down a horizontal body cell, honoring dashed style.
        // Solid segments go through `set_edge` so junctions compose correctly
        // (flowchart/sequence regression); dashed segments use `set` because
        // dashed glyphs do not participate in junction merging.
        let draw_horz = |canvas: &mut Self, x: usize, y: usize| {
            if style.dashed {
                canvas.set(x, y, horz_ch, fg);
            } else {
                canvas.set_edge(x, y, horz_ch, fg);
            }
        };
        let draw_vert_at = |canvas: &mut Self, x: usize, y: usize| {
            if style.dashed {
                canvas.set(x, y, vert_ch, fg);
            } else {
                canvas.set_edge(x, y, vert_ch, fg);
            }
        };

        if src_cy == dst_cy {
            // Straight right. Reserve the first column for a tail glyph and
            // the last column for a head glyph.
            let x_first = src_right_x + 1;
            let x_last = dst_left_x - 1;
            let body_lo = if tail_ch.is_some() { x_first + 1 } else { x_first };
            let body_hi = if head_ch.is_some() { x_last } else { dst_left_x };
            for x in body_lo..body_hi {
                draw_horz(self, x, src_cy);
            }
            if let Some(tc) = tail_ch {
                self.set(x_first, src_cy, tc, fg);
            }
            if let Some(hc) = head_ch {
                self.set(x_last, dst_cy, hc, fg);
            }

            // Label above the horizontal line
            if let Some(text) = style.label {
                let label_x = src_right_x + 2;
                let label_y = if src_cy > 0 { src_cy - 1 } else { 0 };
                for (i, ch) in text.chars().enumerate() {
                    self.set(label_x + i, label_y, ch, fg);
                }
            }
            // Far label one row below the line, near the source end.
            if let Some(text) = style.far_label {
                let label_y = src_cy + 1;
                let label_x = src_right_x + 2;
                for (i, ch) in text.chars().enumerate() {
                    let lx = label_x + i;
                    if lx < self.width {
                        self.set(lx, label_y, ch, fg);
                    }
                }
            }
        } else {
            // Right from source to mid_x. Reserve first column for tail glyph.
            let src_body_lo = if tail_ch.is_some() {
                src_right_x + 2
            } else {
                src_right_x + 1
            };
            for x in src_body_lo..mid_x {
                draw_horz(self, x, src_cy);
            }
            if let Some(tc) = tail_ch {
                self.set(src_right_x + 1, src_cy, tc, fg);
            }

            // Rounded turn at source lane
            let src_turn = if dst_cy > src_cy { '╮' } else { '╯' };
            self.set(mid_x, src_cy, src_turn, fg);

            // Vertical segment
            if src_cy < dst_cy {
                for y in (src_cy + 1)..dst_cy {
                    draw_vert_at(self, mid_x, y);
                }
            } else {
                for y in (dst_cy + 1)..src_cy {
                    draw_vert_at(self, mid_x, y);
                }
            }

            // Rounded turn toward destination
            let dst_turn = if dst_cy > src_cy { '╰' } else { '╭' };
            self.set(mid_x, dst_cy, dst_turn, fg);

            // Right from mid_x to destination. Reserve last column for head.
            let dst_body_hi = if head_ch.is_some() { dst_left_x - 1 } else { dst_left_x };
            for x in (mid_x + 1)..dst_body_hi {
                draw_horz(self, x, dst_cy);
            }
            if let Some(hc) = head_ch {
                self.set(dst_left_x - 1, dst_cy, hc, fg);
            }

            // Label near the vertical segment
            if let Some(text) = style.label {
                let (min_y, max_y) = if src_cy < dst_cy {
                    (src_cy, dst_cy)
                } else {
                    (dst_cy, src_cy)
                };
                let label_y = min_y + (max_y - min_y).saturating_sub(1) / 2;
                for (i, ch) in text.chars().enumerate() {
                    self.set(mid_x + 2 + i, label_y, ch, fg);
                }
            }
            // Far label on the other side of the vertical segment, near source.
            if let Some(text) = style.far_label {
                let (min_y, max_y) = if src_cy < dst_cy {
                    (src_cy, dst_cy)
                } else {
                    (dst_cy, src_cy)
                };
                let label_y = min_y + (max_y - min_y) / 2;
                if mid_x >= 2 + text.chars().count() {
                    let label_x = mid_x - 2 - text.chars().count();
                    for (i, ch) in text.chars().enumerate() {
                        self.set(label_x + i, label_y, ch, fg);
                    }
                }
            }
        }
    }

    pub(crate) fn draw_tree_edge(
        &mut self,
        parent_outer_x: usize,
        parent_cy: usize,
        child_outer_x: usize,
        child_cy: usize,
        fg: Option<Color>,
    ) {
        if parent_outer_x == child_outer_x {
            if parent_cy != child_cy {
                let (top, bot) = if parent_cy < child_cy {
                    (parent_cy + 1, child_cy)
                } else {
                    (child_cy + 1, parent_cy)
                };
                for y in top..bot {
                    self.set_edge(parent_outer_x, y, '│', fg);
                }
            }
            return;
        }

        let going_right = parent_outer_x < child_outer_x;
        let src_x = if going_right {
            parent_outer_x + 1
        } else {
            parent_outer_x.saturating_sub(1)
        };
        let dst_x = if going_right {
            child_outer_x.saturating_sub(1)
        } else {
            child_outer_x + 1
        };

        if parent_cy == child_cy {
            let (min_x, max_x) = if src_x < dst_x {
                (src_x, dst_x)
            } else {
                (dst_x, src_x)
            };
            for x in min_x..=max_x {
                self.set_edge(x, parent_cy, '─', fg);
            }
            return;
        }

        let child_below = child_cy > parent_cy;
        let (src_corner, dst_corner) = if going_right {
            if child_below {
                ('╮', '╰')
            } else {
                ('╯', '╭')
            }
        } else if child_below {
            ('╭', '╯')
        } else {
            ('╰', '╮')
        };

        let mid_x = (src_x + dst_x) / 2;

        if going_right {
            for x in src_x..mid_x {
                self.set_edge(x, parent_cy, '─', fg);
            }
        } else {
            for x in (mid_x + 1)..=src_x {
                self.set_edge(x, parent_cy, '─', fg);
            }
        }

        self.set_edge(mid_x, parent_cy, src_corner, fg);

        if child_below {
            for y in (parent_cy + 1)..child_cy {
                self.set_edge(mid_x, y, '│', fg);
            }
        } else {
            for y in (child_cy + 1)..parent_cy {
                self.set_edge(mid_x, y, '│', fg);
            }
        }

        self.set_edge(mid_x, child_cy, dst_corner, fg);

        if going_right {
            for x in (mid_x + 1)..=dst_x {
                self.set_edge(x, child_cy, '─', fg);
            }
        } else {
            for x in dst_x..mid_x {
                self.set_edge(x, child_cy, '─', fg);
            }
        }
    }

    /// Lay down a 2-character crow's-foot cardinality decoration at endpoint
    /// `(x, y)`. `dir` selects which way the decoration faces: `Down` hangs
    /// below an entity (source end of a TD edge), `Up` rises above (dest end).
    /// Uses `set_edge` so the glyphs compose with edge-crossing junctions.
    pub(crate) fn draw_crowsfoot(
        &mut self,
        x: usize,
        y: usize,
        dir: CrowDir,
        card: Card,
        fg: Option<Color>,
    ) {
        let (near_entity, near_trunk) = crowsfoot_chars(card, dir);
        match dir {
            CrowDir::Down => {
                self.set_edge(x, y, near_entity, fg);
                self.set_edge(x, y + 1, near_trunk, fg);
            }
            CrowDir::Up => {
                self.set_edge(x, y, near_entity, fg);
                if y > 0 {
                    self.set_edge(x, y - 1, near_trunk, fg);
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn draw_tree_edge_right_going_uses_rounded_z() {
        let mut canvas = Canvas::new(20, 10);
        canvas.draw_tree_edge(6, 5, 14, 7, None);

        assert_eq!(canvas.cells[5][10].ch, '╮', "source corner (right then down)");
        assert_eq!(canvas.cells[7][10].ch, '╰', "destination corner (up then right)");
        for x in 7..10 {
            assert_eq!(canvas.cells[5][x].ch, '─', "horizontal near parent at x={x}");
        }
        assert_eq!(canvas.cells[6][10].ch, '│', "vertical segment");
        for x in 11..14 {
            assert_eq!(canvas.cells[7][x].ch, '─', "horizontal near child at x={x}");
        }
    }

    #[test]
    fn draw_tree_edge_left_going_mirrored() {
        let mut canvas = Canvas::new(20, 10);
        canvas.draw_tree_edge(14, 5, 6, 7, None);

        assert_eq!(canvas.cells[5][10].ch, '╭', "source corner (left then down)");
        assert_eq!(canvas.cells[7][10].ch, '╯', "destination corner (up then left)");
        for x in 11..14 {
            assert_eq!(canvas.cells[5][x].ch, '─', "horizontal near parent at x={x}");
        }
        assert_eq!(canvas.cells[6][10].ch, '│', "vertical segment");
        for x in 7..10 {
            assert_eq!(canvas.cells[7][x].ch, '─', "horizontal near child at x={x}");
        }
    }

    #[test]
    fn draw_tree_edge_same_row_is_straight_horizontal() {
        let mut canvas = Canvas::new(20, 5);
        canvas.draw_tree_edge(5, 2, 15, 2, None);
        for x in 6..15 {
            assert_eq!(canvas.cells[2][x].ch, '─', "straight horizontal at x={x}");
        }
    }
}
