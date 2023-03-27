use std::cmp::{max, min, Ordering};

use crate::text_utils::{self, CharType};

#[derive(Clone, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    cached_col: usize,
}

impl Cursor {
    pub fn new(row: usize, col: usize) -> Self {
        Self {
            row,
            col,
            cached_col: col,
        }
    }

    pub fn move_down(&mut self, lines: &[Vec<u8>], count: usize) {
        self.cached_col = max(self.cached_col, self.col);
        self.row = min(self.row + count, lines.len().saturating_sub(1));
        self.col = min(
            max(self.cached_col, self.col),
            lines[self.row].len().saturating_sub(1),
        );
    }

    pub fn move_up(&mut self, lines: &[Vec<u8>], count: usize) {
        self.cached_col = max(self.cached_col, self.col);
        self.row = self.row.saturating_sub(count);
        self.col = min(
            max(self.cached_col, self.col),
            lines[self.row].len().saturating_sub(1),
        );
    }

    pub fn move_forward(&mut self, lines: &[Vec<u8>], count: usize) {
        self.cached_col = 0;
        self.col = min(self.col + count, lines[self.row].len().saturating_sub(1));
    }

    pub fn move_backward(&mut self, count: usize) {
        self.cached_col = 0;
        self.col = self.col.saturating_sub(count);
    }

    pub fn move_forward_by_word(&mut self, lines: &[Vec<u8>]) {
        let count = self.chars_until_word_boundary(lines);
        self.move_forward(lines, count);
    }

    pub fn move_backward_by_word(&mut self, lines: &[Vec<u8>]) {
        let count = self.chars_until_word_boundary_rev(lines);
        self.move_backward(count);
    }

    pub fn move_to_start_of_line(&mut self) {
        self.cached_col = 0;
        self.col = 0;
    }

    pub fn move_to_end_of_line(&mut self, lines: &[Vec<u8>]) {
        self.cached_col = lines[self.row].len().saturating_sub(1);
        self.col = lines[self.row].len().saturating_sub(1);
    }

    pub fn move_to_start_of_file(&mut self) {
        self.cached_col = 0;
        self.row = 0;
        self.col = 0;
    }

    pub fn move_to_end_of_file(&mut self, lines: &[Vec<u8>]) {
        self.cached_col = lines[self.row].len().saturating_sub(1);
        self.row = lines.len().saturating_sub(1);
        self.col = lines[self.row].len().saturating_sub(1);
    }

    pub fn move_forward_to_char(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char(lines, search_char);
        self.move_forward(lines, count);
    }

    pub fn move_backward_to_char(&mut self, lines: &[Vec<u8>], search_char: u8) {
        let count = self.chars_until_char_rev(lines, search_char);
        self.move_backward(count);
    }

    fn chars_until_char(&self, lines: &[Vec<u8>], search_char: u8) -> usize {
        if self.col == lines[self.row].len() {
            return 0;
        }

        let mut count = 0;
        for c in lines[self.row][self.col + 1..].iter() {
            count += 1;
            if c == &search_char {
                return count;
            }
        }
        0
    }

    fn chars_until_char_rev(&self, lines: &[Vec<u8>], search_char: u8) -> usize {
        if self.col == 0 {
            return 0;
        }

        let mut count = 0;
        for c in lines[self.row][..self.col].iter().rev() {
            count += 1;
            if c == &search_char {
                return count;
            }
        }
        0
    }

    fn chars_until_word_boundary(&self, lines: &[Vec<u8>]) -> usize {
        if lines[self.row].is_empty() {
            return 0;
        }

        let current_char_type = text_utils::get_ascii_char_type(lines[self.row][self.col]);

        let mut count = 0;
        let mut separator_found = false;
        for c in lines[self.row][self.col..].iter() {
            let char_type = text_utils::get_ascii_char_type(*c);
            separator_found |= current_char_type != char_type;
            if separator_found && char_type != CharType::Whitespace {
                break;
            }
            count += 1;
        }
        count
    }

    fn chars_until_word_boundary_rev(&self, lines: &[Vec<u8>]) -> usize {
        if lines[self.row].is_empty() {
            return 0;
        }

        let mut current_char_type = None;
        let mut count = 0;
        for c in lines[self.row][..self.col].iter().rev() {
            let char_type = text_utils::get_ascii_char_type(*c);
            if char_type != CharType::Whitespace && current_char_type.is_none() {
                current_char_type = Some(char_type);
            }

            if let Some(current_char_type) = current_char_type {
                if char_type != current_char_type {
                    break;
                }
            }

            count += 1;
        }
        count
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
