use std::{
    cmp::{max, min},
    ops::Range,
};

use crate::{
    piece_table::PieceTable,
    text_utils::{self, CharType},
};

#[derive(Copy, Clone, Debug)]
pub struct Cursor {
    pub position: usize,
    pub anchor: usize,
    pub cached_col: usize,
    pub completion_request: Option<CompletionRequest>,
}

pub const NUM_SHOWN_COMPLETION_ITEMS: usize = 15;
#[derive(Copy, Clone, Debug)]
pub struct CompletionRequest {
    pub id: i32,
    pub position: usize,
    pub selection_index: usize,
    pub selection_view_offset: usize,
}

#[derive(Debug)]
pub struct SelectionRange {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

pub fn cursors_overlapping(c1: &Cursor, c2: &Cursor) -> bool {
    min(c1.position, c1.anchor) <= max(c2.position, c2.anchor)
        && min(c2.position, c2.anchor) <= max(c1.position, c1.anchor)
}

pub fn cursors_foreach_rebalance<F>(mut cursors: &mut [Cursor], mut f: F)
where
    F: FnMut(&mut Cursor),
{
    for i in 0..cursors.len() {
        let cursor_before = cursors[i];
        f(&mut cursors[i]);

        for j in 0..cursors.len() {
            if i == j {
                continue;
            }

            if cursor_before.single_selection() {
                if cursor_before.position < cursors[j].position {
                    let delta = cursors[i].position as isize - cursor_before.position as isize;
                    cursors[j].anchor = cursors[j].anchor.saturating_add_signed(delta);
                    cursors[j].position = cursors[j].position.saturating_add_signed(delta);
                }
            } else if cursor_before.position < cursors[j].position {
                let delta = cursor_before.position.abs_diff(cursor_before.anchor) + 1;
                cursors[j].anchor -= delta;
                cursors[j].position -= delta;
            }
        }
    }
}

impl Cursor {
    pub fn new(position: usize) -> Self {
        Self {
            position,
            anchor: position,
            cached_col: 0,
            completion_request: None,
        }
    }

    pub fn range(&self) -> Range<usize> {
        min(self.position, self.anchor)..max(self.position, self.anchor)
    }

    pub fn default() -> Self {
        Self {
            position: 0,
            anchor: 0,
            cached_col: 0,
            completion_request: None,
        }
    }

    pub fn stick_col(&mut self, piece_table: &PieceTable) {
        self.cached_col = max(self.cached_col, piece_table.col_index(self.position));
    }

    pub fn unstick_col(&mut self, piece_table: &PieceTable) {
        self.cached_col = piece_table.col_index(self.position);
    }

    pub fn move_down(&mut self, piece_table: &PieceTable, count: usize) {
        let index = piece_table.line_index(self.position);
        if let Some(line) = piece_table.line_at_index(index + 1) {
            let col = piece_table.col_index(self.position);
            self.position =
                line.start + min(max(col, self.cached_col), line.length.saturating_sub(1));
        }
    }

    pub fn move_up(&mut self, piece_table: &PieceTable, count: usize) {
        let index = piece_table.line_index(self.position);
        if index == 0 {
            return;
        }

        if let Some(line) = piece_table.line_at_index(index - 1) {
            let col = piece_table.col_index(self.position);
            self.position =
                line.start + min(max(col, self.cached_col), line.length.saturating_sub(1));
        }
    }

    pub fn move_forward(&mut self, piece_table: &PieceTable, count: usize) {
        let c = piece_table.char_at(self.position);
        if c.is_none() || c.is_some_and(|c| c == b'\n') {
            return;
        }

        if let Some(chars_until_newline) = self.chars_until_char(piece_table, b'\n') {
            self.position += min(count, chars_until_newline + 1);
        } else {
            self.position += count;
        }
    }

    pub fn move_forward_once_wrapping(&mut self, piece_table: &PieceTable) {
        self.position = min(self.position + 1, piece_table.num_chars().saturating_sub(1));
    }

    pub fn move_backward(&mut self, piece_table: &PieceTable, count: usize) {
        if let Some(chars_until_newline) = self.chars_until_char_rev(piece_table, b'\n') {
            self.position -= min(count, chars_until_newline);
        } else {
            self.position = self.position.saturating_sub(count);
        }
    }

    pub fn move_forward_by_word(&mut self, piece_table: &PieceTable) {
        let mut count = 0;
        for (c1, c2) in piece_table
            .iter_chars_at(self.position)
            .zip(piece_table.iter_chars_at(self.position).skip(1))
        {
            count += 1;
            let type1 = text_utils::char_type(c1);
            let type2 = text_utils::char_type(c2);

            // Special case: empty line is considered a word
            if (c1 == b'\n' && c2 == b'\n') || (type2 != CharType::Whitespace && type1 != type2) {
                self.position += count;
                return;
            }
        }
        self.position = piece_table.num_chars().saturating_sub(1);
    }

    pub fn move_backward_by_word(&mut self, piece_table: &PieceTable) {
        let mut count = 0;
        for (c1, c2) in piece_table
            .iter_chars_at_rev(self.position.saturating_sub(1))
            .zip(
                piece_table
                    .iter_chars_at_rev(self.position.saturating_sub(1))
                    .skip(1),
            )
        {
            count += 1;
            let type1 = text_utils::char_type(c1);
            let type2 = text_utils::char_type(c2);

            // Special case: empty line is considered a word
            if (c1 == b'\n' && c2 == b'\n') || (type1 != CharType::Whitespace && type1 != type2) {
                self.position -= count;
                return;
            }
        }
        self.position = 0;
    }

    pub fn move_to_start_of_line(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.position = line.start;
        }
    }

    pub fn move_to_end_of_line(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.position = line.end;
        }
    }

    pub fn move_to_start_of_file(&mut self) {
        self.position = 0;
    }

    pub fn move_to_end_of_file(&mut self, piece_table: &PieceTable) {
        self.position = piece_table.num_chars().saturating_sub(1);
    }

    pub fn move_to_char_inc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char(piece_table, search_char) {
            self.move_forward(piece_table, count + 1);
        }
    }

    pub fn move_back_to_char_inc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char_rev(piece_table, search_char) {
            self.move_backward(piece_table, count + 1);
        }
    }

    pub fn move_to_char_exc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char(piece_table, search_char) {
            self.move_forward(piece_table, count);
        }
    }

    pub fn move_back_to_char_exc(&mut self, piece_table: &PieceTable, search_char: u8) {
        if let Some(count) = self.chars_until_char_rev(piece_table, search_char) {
            self.move_backward(piece_table, count);
        }
    }

    pub fn move_to_first_non_blank_char(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.position = line.start;
            if line.length > 1 {
                if let Some(count) =
                    self.chars_until_pred(piece_table, |c| !c.is_ascii_whitespace() || c == b'\n')
                {
                    self.move_forward(piece_table, count);
                }
            }
        }
    }

    pub fn select_line(&mut self, piece_table: &PieceTable) {
        if let Some(line) = piece_table.line_at_char(self.position) {
            self.anchor = line.start;
            self.position = line.end;
        }
    }

    pub fn reset_anchor(&mut self) {
        self.anchor = self.position;
    }

    pub fn get_selection_ranges(&self, piece_table: &PieceTable) -> Vec<SelectionRange> {
        let line = piece_table.line_index(self.position);
        let col = piece_table.col_index(self.position);
        let anchor_line = piece_table.line_index(self.anchor);
        let anchor_col = piece_table.col_index(self.anchor);

        if line == anchor_line {
            vec![SelectionRange {
                line,
                start: min(col, anchor_col),
                end: max(col, anchor_col),
            }]
        } else {
            let (first_line, first_col, last_line, last_col) = if self.position < self.anchor {
                (line, col, anchor_line, anchor_col)
            } else {
                (anchor_line, anchor_col, line, col)
            };

            let mut ranges = vec![];
            ranges.push(SelectionRange {
                line: first_line,
                start: first_col,
                end: piece_table.line_at_index(first_line).unwrap().length,
            });

            for line in first_line + 1..last_line {
                ranges.push(SelectionRange {
                    line,
                    start: 0,
                    end: piece_table.line_at_index(line).unwrap().length,
                });
            }

            ranges.push(SelectionRange {
                line: last_line,
                start: 0,
                end: last_col,
            });
            ranges
        }
    }

    pub fn at_line_end(&self, piece_table: &PieceTable) -> bool {
        piece_table
            .line_at_char(self.position)
            .is_some_and(|line| line.end == self.position)
    }

    pub fn moving_forward(&self) -> bool {
        self.position >= self.anchor
    }

    pub fn get_line_col(&self, piece_table: &PieceTable) -> (usize, usize) {
        (
            piece_table.line_index(self.position),
            piece_table.col_index(self.position),
        )
    }

    fn single_selection(&self) -> bool {
        self.position == self.anchor
    }

    fn chars_until_pred<F>(&self, piece_table: &PieceTable, pred: F) -> Option<usize>
    where
        F: Fn(u8) -> bool,
    {
        piece_table.iter_chars_at(self.position + 1).position(pred)
    }

    fn chars_until_pred_rev<F>(&self, piece_table: &PieceTable, pred: F) -> Option<usize>
    where
        F: Fn(u8) -> bool,
    {
        piece_table
            .iter_chars_at_rev(self.position.saturating_sub(1))
            .position(pred)
    }

    fn chars_until_char(&self, piece_table: &PieceTable, search_char: u8) -> Option<usize> {
        self.chars_until_pred(piece_table, |c| c == search_char)
    }

    fn chars_until_char_rev(&self, piece_table: &PieceTable, search_char: u8) -> Option<usize> {
        self.chars_until_pred_rev(piece_table, |c| c == search_char)
    }
}
