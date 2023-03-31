use std::cmp::{max, min, Ordering};

use crate::text_utils::{self, CharType};

#[derive(Copy, Clone, Eq, Debug)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub anchor_row: usize,
    pub anchor_col: usize,
    pub cached_col: usize,
}

pub struct SelectionRange {
    pub row: usize,
    pub start: usize,
    pub end: usize,
}

pub fn cursors_overlapping(c1: &Cursor, c2: &Cursor) -> bool {
    c1.overlaps(c2) || c2.overlaps(c1)
}

impl Cursor {
    pub fn new(row: usize, col: usize) -> Self {
        Self {
            row,
            col,
            anchor_row: row,
            anchor_col: col,
            cached_col: col,
        }
    }

    pub fn stick_col(&mut self) {
        self.cached_col = max(self.cached_col, self.col);
    }

    pub fn unstick_col(&mut self) {
        self.cached_col = self.col;
    }

    pub fn move_down(&mut self, lines: &[Vec<u8>], count: usize) {
        self.row = min(self.row + count, lines.len().saturating_sub(1));
        self.col = min(
            max(self.cached_col, self.col),
            self.line_zero_indexed_length(lines),
        );
    }

    pub fn move_up(&mut self, lines: &[Vec<u8>], count: usize) {
        self.row = self.row.saturating_sub(count);
        self.col = min(
            max(self.cached_col, self.col),
            self.line_zero_indexed_length(lines),
        );
    }

    pub fn move_forward(&mut self, lines: &[Vec<u8>], count: usize) {
        self.col = min(self.col + count, self.line_zero_indexed_length(lines));
    }

    pub fn move_backward(&mut self, lines: &[Vec<u8>], count: usize) {
        self.col = self.col.saturating_sub(count);
    }

    pub fn move_forward_by_word(&mut self, lines: &[Vec<u8>]) {
        let line_iterator = lines[self.row][self.col..].iter();
        let next_line_iterator = if self.row < lines.len().saturating_sub(1) {
            lines[self.row + 1].iter()
        } else {
            [].iter()
        };

        let current_char_type = if lines[self.row].is_empty() {
            CharType::Whitespace
        } else {
            text_utils::get_ascii_char_type(lines[self.row][self.col])
        };
        let mut count = if lines[self.row].is_empty() { 1 } else { 0 };
        let mut separator_found = false;
        for c in line_iterator.chain(next_line_iterator) {
            let char_type = text_utils::get_ascii_char_type(*c);
            separator_found |= current_char_type != char_type;
            if separator_found && char_type != CharType::Whitespace {
                break;
            }
            count += 1;
        }

        // Move forward, going to the line below if necessary
        let line_length = lines[self.row].len().saturating_sub(1);
        if self.col + count > line_length && self.row < lines.len().saturating_sub(1) {
            self.row += 1;
            self.col = (count - (line_length - self.col)).saturating_sub(1);
        } else {
            self.col = min(self.col + count, line_length);
        }
    }

    pub fn move_backward_by_word(&mut self, lines: &[Vec<u8>]) {
        let line_iterator = if !lines[self.row].is_empty() {
            lines[self.row][..=self.col].iter().rev()
        } else {
            [].iter().rev()
        };
        let prev_line_iterator = if self.row > 0 {
            lines[self.row - 1].iter().rev()
        } else {
            [].iter().rev()
        };

        let current_char_type = if lines[self.row].is_empty() {
            CharType::Whitespace
        } else {
            text_utils::get_ascii_char_type(lines[self.row][self.col])
        };
        let mut count = if lines[self.row].is_empty() { 1 } else { 0 };
        let mut separator_found = false;
        for c in line_iterator.chain(prev_line_iterator) {
            let char_type = text_utils::get_ascii_char_type(*c);
            separator_found |= current_char_type != char_type;
            if separator_found && char_type != CharType::Whitespace {
                break;
            }
            count += 1;
        }

        // Move backward, going to the line above if necessary
        if self.col < count && self.row > 0 {
            self.row -= 1;
            self.col = lines[self.row].len().saturating_sub(count - self.col);
        } else {
            self.col = self.col.saturating_sub(count);
        }

        self.move_to_start_of_word(lines)
    }

    pub fn move_to_start_of_word(&mut self, lines: &[Vec<u8>]) {
        let char_type = if lines[self.row].is_empty() {
            CharType::Whitespace
        } else {
            text_utils::get_ascii_char_type(lines[self.row][self.col])
        };
        if let Some(count) =
            self.chars_until_pred_rev(lines, |c| text_utils::get_ascii_char_type(c) != char_type)
        {
            self.move_backward(lines, count.saturating_sub(1));
        } else {
            self.move_to_start_of_line();
        }
    }

    pub fn move_to_start_of_line(&mut self) {
        self.col = 0;
    }

    pub fn move_to_end_of_line(&mut self, lines: &[Vec<u8>]) {
        self.col = self.line_zero_indexed_length(lines);
    }

    pub fn move_to_start_of_file(&mut self) {
        self.row = 0;
        self.col = 0;
    }

    pub fn move_to_end_of_file(&mut self, lines: &[Vec<u8>]) {
        self.row = lines.len().saturating_sub(1);
        self.col = self.line_zero_indexed_length(lines);
    }

    pub fn move_forward_to_char(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char(lines, search_char);
        self.move_forward(lines, count);
    }

    pub fn move_backward_to_char(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char_rev(lines, search_char);
        self.move_backward(lines, count);
    }

    pub fn move_to_first_non_blank_char(&mut self, lines: &[Vec<u8>]) {
        let mut col = 0;
        for c in &lines[self.row] {
            if !c.is_ascii_whitespace() {
                break;
            }
            col += 1;
        }
        self.col = col
    }

    pub fn reset_anchor(&mut self) {
        self.anchor_row = self.row;
        self.anchor_col = self.col;
    }

    pub fn get_selection_ranges(&self, lines: &[Vec<u8>]) -> Vec<SelectionRange> {
        let mut ranges = vec![];
        if self.row == self.anchor_row {
            let first_col = min(self.col, self.anchor_col);
            let last_col = max(self.col, self.anchor_col);
            ranges.push(SelectionRange {
                row: self.row,
                start: first_col,
                end: last_col,
            });
        } else {
            let (first_row, first_col, last_row, last_col) = if self.row < self.anchor_row {
                (self.row, self.col, self.anchor_row, self.anchor_col)
            } else {
                (self.anchor_row, self.anchor_col, self.row, self.col)
            };
            ranges.push(SelectionRange {
                row: first_row,
                start: first_col,
                end: lines[first_row].len().saturating_sub(1),
            });
            for row in (first_row + 1)..last_row {
                ranges.push(SelectionRange {
                    row,
                    start: 0,
                    end: lines[row].len().saturating_sub(1),
                });
            }
            ranges.push(SelectionRange {
                row: last_row,
                start: 0,
                end: last_col,
            });
        }
        ranges
    }

    pub fn moving_forward(&self) -> bool {
        self.row > self.anchor_row || (self.row == self.anchor_row && self.col >= self.anchor_col)
    }

    fn overlaps(&self, other: &Cursor) -> bool {
        if self.moving_forward() {
            (self.row > other.anchor_row && self.anchor_row < other.anchor_row)
                || (self.row == other.anchor_row && self.col >= other.anchor_col)
        } else {
            (self.row < other.anchor_row && self.anchor_row > other.anchor_row)
                || (self.row == other.anchor_row && self.col <= other.anchor_col)
        }
    }

    fn line_zero_indexed_length(&self, lines: &[Vec<u8>]) -> usize {
        lines[self.row].len().saturating_sub(1)
    }

    fn chars_until_pred<F>(&self, lines: &[Vec<u8>], pred: F) -> Option<usize>
    where
        F: Fn(u8) -> bool,
    {
        if let Some(line) = lines.get(self.row) {
            let mut count = 0;
            for c in line[self.col + 1..].iter() {
                count += 1;
                if pred(*c) {
                    return Some(count);
                }
            }
        }

        None
    }

    fn chars_until_pred_rev<F>(&self, lines: &[Vec<u8>], pred: F) -> Option<usize>
    where
        F: Fn(u8) -> bool,
    {
        if let Some(line) = lines.get(self.row) {
            let mut count = 0;
            for c in line[..self.col].iter().rev() {
                count += 1;
                if pred(*c) {
                    return Some(count);
                }
            }
        }
        None
    }

    fn chars_until_char(&self, lines: &[Vec<u8>], search_char: u8) -> usize {
        self.chars_until_pred(lines, |c| c == search_char)
            .unwrap_or(0)
    }

    fn chars_until_char_rev(&self, lines: &[Vec<u8>], search_char: u8) -> usize {
        self.chars_until_pred_rev(lines, |c| c == search_char)
            .unwrap_or(0)
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.row.cmp(&other.row).then(self.col.cmp(&other.col))
    }
}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row && self.col == other.col
    }
}
