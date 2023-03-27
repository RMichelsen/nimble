use std::{
    cell::RefCell,
    cmp::{max, min},
    rc::Rc,
};

use crate::text_utils::{self, CharType};

#[derive(Clone)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    cached_col: usize,
    lines: Rc<RefCell<Vec<Vec<u8>>>>,
}

impl Cursor {
    pub fn new(row: usize, col: usize, lines: Rc<RefCell<Vec<Vec<u8>>>>) -> Self {
        Self {
            row,
            col,
            cached_col: col,
            lines,
        }
    }

    pub fn move_down(&mut self, count: usize) {
        let lines = self.lines.borrow();
        self.cached_col = max(self.cached_col, self.col);
        self.row = min(self.row + count, lines.len().saturating_sub(1));
        self.col = min(
            max(self.cached_col, self.col),
            lines[self.row].len().saturating_sub(1),
        );
    }

    pub fn move_up(&mut self, count: usize) {
        self.cached_col = max(self.cached_col, self.col);
        self.row = self.row.saturating_sub(count);
        self.col = min(
            max(self.cached_col, self.col),
            self.lines.borrow()[self.row].len().saturating_sub(1),
        );
    }

    pub fn move_forward(&mut self, count: usize) {
        self.cached_col = 0;
        self.col = min(
            self.col + count,
            self.lines.borrow()[self.row].len().saturating_sub(1),
        );
    }

    pub fn move_backward(&mut self, count: usize) {
        self.cached_col = 0;
        self.col = self.col.saturating_sub(count);
    }

    pub fn move_forward_by_word(&mut self) {
        let count = self.chars_until_word_boundary();
        self.move_forward(count);
    }

    pub fn move_backward_by_word(&mut self) {
        let count = self.chars_until_word_boundary_rev();
        self.move_backward(count);
    }

    pub fn move_to_start_of_line(&mut self) {
        self.cached_col = 0;
        self.col = 0;
    }

    pub fn move_to_end_of_line(&mut self) {
        let lines = self.lines.borrow();
        self.cached_col = lines[self.row].len().saturating_sub(1);
        self.col = lines[self.row].len().saturating_sub(1);
    }

    pub fn move_to_start_of_file(&mut self) {
        self.cached_col = 0;
        self.row = 0;
        self.col = 0;
    }

    pub fn move_to_end_of_file(&mut self) {
        let lines = self.lines.borrow();
        self.cached_col = lines[self.row].len().saturating_sub(1);
        self.row = lines.len().saturating_sub(1);
        self.col = lines[self.row].len().saturating_sub(1);
    }

    pub fn move_forward_to_char(&mut self, search_char: u8) {
        let count = self.chars_until_char(search_char);
        self.move_forward(count);
    }

    pub fn move_backward_to_char(&mut self, search_char: u8) {
        let count = self.chars_until_char_rev(search_char);
        self.move_backward(count);
    }

    fn chars_until_char(&self, search_char: u8) -> usize {
        let lines = self.lines.borrow();
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

    fn chars_until_char_rev(&self, search_char: u8) -> usize {
        let lines = self.lines.borrow();
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

    fn chars_until_word_boundary(&self) -> usize {
        let lines = self.lines.borrow();
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

    fn chars_until_word_boundary_rev(&self) -> usize {
        let lines = self.lines.borrow();
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
