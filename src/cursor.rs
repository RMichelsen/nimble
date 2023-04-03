use std::cmp::{max, min, Ordering};

use crate::text_utils::{self, CharType};

#[derive(Copy, Clone, Eq, Debug)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    pub anchor_line: usize,
    pub anchor_col: usize,
    pub cached_col: usize,
    pub completion_active: bool,
}

#[derive(Debug)]
pub struct SelectionRange {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

pub enum LineChange {
    Inserted(usize),
    Removed(usize),
}

#[derive(Debug)]
pub enum ColChange {
    Inserted(usize, usize),
    Removed(usize, usize),
}

pub struct RemoveCols {
    pub line: usize,
}

pub fn cursors_overlapping(c1: &Cursor, c2: &Cursor) -> bool {
    c1.overlaps(c2) || c2.overlaps(c1)
}

pub fn cursors_foreach_rebalance<F>(mut cursors: &mut [Cursor], mut f: F)
where
    F: FnMut(&mut Cursor) -> (Vec<LineChange>, Vec<ColChange>),
{
    while let Some((cursor, tail)) = cursors.split_first_mut() {
        let (line_changes, col_changes) = f(cursor);
        rebalance_cursors(tail, &line_changes, &col_changes);
        cursors = tail;
    }
}

pub fn rebalance_cursors(
    cursors: &mut [Cursor],
    line_changes: &[LineChange],
    col_changes: &[ColChange],
) {
    for cursor in cursors {
        for change in line_changes {
            match change {
                LineChange::Inserted(line) => {
                    let offset = (cursor.line >= *line) as usize;
                    cursor.line += offset;
                    cursor.anchor_line += offset;
                }
                LineChange::Removed(line) => {
                    let offset = (cursor.line >= *line) as usize;
                    cursor.line -= offset;
                    cursor.anchor_line -= offset;
                }
            }
        }
        for change in col_changes {
            match change {
                ColChange::Inserted(line, num) if cursor.line == *line => {
                    cursor.col += *num;
                }
                ColChange::Inserted(line, num) if cursor.anchor_line == *line => {
                    cursor.anchor_col += *num;
                }
                ColChange::Removed(line, num) if cursor.line == *line => {
                    cursor.col -= *num;
                }
                ColChange::Removed(line, num) if cursor.anchor_line == *line => {
                    cursor.anchor_col -= *num;
                }
                _ => (),
            }
        }
    }
}

impl Cursor {
    pub fn new(line: usize, col: usize) -> Self {
        Self {
            line,
            col,
            anchor_line: line,
            anchor_col: col,
            cached_col: col,
            completion_active: true,
        }
    }

    pub fn stick_col(&mut self) {
        self.cached_col = max(self.cached_col, self.col);
    }

    pub fn unstick_col(&mut self) {
        self.cached_col = self.col;
    }

    pub fn move_down(&mut self, lines: &[Vec<u8>], count: usize) {
        self.line = min(self.line + count, lines.len().saturating_sub(1));
        self.col = min(
            max(self.cached_col, self.col),
            self.line_zero_indexed_length(lines),
        );
    }

    pub fn move_up(&mut self, lines: &[Vec<u8>], count: usize) {
        self.line = self.line.saturating_sub(count);
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
        let mut count = 0;
        for chars in lines[self.line][self.col..].windows(2) {
            count += 1;
            let type1 = text_utils::get_ascii_char_type(chars[0]);
            let type2 = text_utils::get_ascii_char_type(chars[1]);

            if type2 != CharType::Whitespace && type1 != type2 {
                self.move_forward(lines, count);
                return;
            }
        }

        if self.line < lines.len().saturating_sub(1) {
            self.line += 1;
        }
        self.move_to_first_non_blank_char(lines);
    }

    pub fn move_backward_by_word(&mut self, lines: &[Vec<u8>]) {
        let mut count = 0;
        for chars in lines[self.line][..self.col].windows(2).rev() {
            count += 1;
            let type1 = text_utils::get_ascii_char_type(chars[0]);
            let type2 = text_utils::get_ascii_char_type(chars[1]);

            if type2 != CharType::Whitespace && type1 != type2 {
                self.move_backward(lines, count);
                return;
            }
        }

        if self.col != 0 && !lines[self.line][0].is_ascii_whitespace() {
            self.col = 0;
            return;
        }

        self.line = self.line.saturating_sub(1);
        self.move_to_last_non_blank_char(lines);
        self.move_to_start_of_word(lines)
    }

    pub fn move_to_start_of_word(&mut self, lines: &[Vec<u8>]) {
        if lines[self.line].is_empty() {
            return;
        }

        let char_type = text_utils::get_ascii_char_type(lines[self.line][self.col]);
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
        self.line = 0;
        self.col = 0;
    }

    pub fn move_to_end_of_file(&mut self, lines: &[Vec<u8>]) {
        self.line = lines.len().saturating_sub(1);
        self.col = self.line_zero_indexed_length(lines);
    }

    pub fn move_forward_to_char_inclusive(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char(lines, search_char);
        self.move_forward(lines, count);
    }

    pub fn move_backward_to_char_inclusive(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char_rev(lines, search_char);
        self.move_backward(lines, count);
    }

    pub fn move_forward_to_char_exclusive(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char(lines, search_char);
        self.move_forward(lines, count.saturating_sub(1));
    }

    pub fn move_backward_to_char_exclusive(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char_rev(lines, search_char);
        self.move_backward(lines, count.saturating_sub(1));
    }

    pub fn move_to_first_non_blank_char(&mut self, lines: &[Vec<u8>]) {
        let mut col = 0;
        for c in &lines[self.line] {
            if !c.is_ascii_whitespace() {
                break;
            }
            col += 1;
        }
        self.col = col
    }

    pub fn move_to_last_non_blank_char(&mut self, lines: &[Vec<u8>]) {
        let mut col = self.line_zero_indexed_length(lines);
        for c in lines[self.line].iter().rev() {
            if !c.is_ascii_whitespace() {
                break;
            }
            col -= 1;
        }
        self.col = col
    }

    pub fn reset_anchor(&mut self) {
        self.anchor_line = self.line;
        self.anchor_col = self.col;
    }

    pub fn get_selection_ranges(&self, lines: &[Vec<u8>]) -> Vec<SelectionRange> {
        let mut ranges = vec![];
        if self.line == self.anchor_line {
            let first_col = min(self.col, self.anchor_col);
            let last_col = max(self.col, self.anchor_col);
            ranges.push(SelectionRange {
                line: self.line,
                start: first_col,
                end: last_col,
            });
        } else {
            let (first_line, first_col, last_line, last_col) = if self.line < self.anchor_line {
                (self.line, self.col, self.anchor_line, self.anchor_col)
            } else {
                (self.anchor_line, self.anchor_col, self.line, self.col)
            };
            ranges.push(SelectionRange {
                line: first_line,
                start: first_col,
                end: lines[first_line].len().saturating_sub(1),
            });

            for (i, line) in lines
                .iter()
                .enumerate()
                .take(last_line)
                .skip(first_line + 1)
            {
                ranges.push(SelectionRange {
                    line: i,
                    start: 0,
                    end: line.len().saturating_sub(1),
                });
            }

            ranges.push(SelectionRange {
                line: last_line,
                start: 0,
                end: last_col,
            });
        }
        ranges
    }

    pub fn line_zero_indexed_length(&self, lines: &[Vec<u8>]) -> usize {
        lines[self.line].len().saturating_sub(1)
    }

    pub fn moving_forward(&self) -> bool {
        self.line > self.anchor_line
            || (self.line == self.anchor_line && self.col >= self.anchor_col)
    }

    fn overlaps(&self, other: &Cursor) -> bool {
        if self.moving_forward() {
            (self.line > other.anchor_line && self.line <= other.line)
                || (self.line == other.anchor_line
                    && (other.anchor_col..=other.col).contains(&self.col))
        } else {
            (self.line < other.anchor_line && self.line >= other.line)
                || (self.line == other.anchor_line
                    && (other.col..=other.anchor_col).contains(&self.col))
        }
    }

    fn chars_until_pred<F>(&self, lines: &[Vec<u8>], pred: F) -> Option<usize>
    where
        F: Fn(u8) -> bool,
    {
        if let Some(line) = lines.get(self.line) {
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
        if let Some(line) = lines.get(self.line) {
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
        self.line.cmp(&other.line).then(self.col.cmp(&other.col))
    }
}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        self.line == other.line && self.col == other.col
    }
}
