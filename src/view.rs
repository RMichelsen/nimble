use std::{
    cell::{Ref, RefCell},
    cmp::min,
    rc::Rc,
};

use crate::{cursor::Cursor, language_support::Language};

pub struct View {
    pub language: Rc<RefCell<Language>>,

    row_offset: usize,
    col_offset: usize,
    num_rows: usize,
    num_cols: usize,
    lines: Rc<RefCell<Vec<Vec<u8>>>>,
    cursors: Rc<RefCell<Vec<Cursor>>>,
}

impl View {
    pub fn new(
        num_rows: usize,
        num_cols: usize,
        lines: Rc<RefCell<Vec<Vec<u8>>>>,
        cursors: Rc<RefCell<Vec<Cursor>>>,
        language: Rc<RefCell<Language>>,
    ) -> Self {
        Self {
            language,

            row_offset: 0,
            col_offset: 0,
            num_rows,
            num_cols,
            lines,
            cursors,
        }
    }

    pub fn visible_cursors_iter<F>(&self, f: F)
    where
        F: Fn(usize, usize),
    {
        for cursor in self.cursors.borrow().iter() {
            if self.pos_in_render_visible_range(cursor.row, cursor.col) {
                let (row, col) = self.absolute_to_view_pos(cursor.row, cursor.col);
                f(row, col);
            }
        }
    }

    pub fn visible_lines_iter<F>(&self, f: F)
    where
        F: Fn(usize, &[u8]),
    {
        let lines = self.lines.borrow();
        for (i, line) in lines[self.row_offset..min(self.row_offset + self.num_rows, lines.len())]
            .iter()
            .enumerate()
        {
            f(i, line);
        }
    }

    pub fn adjust(&mut self) {
        if let Some(first_cursor) = self.cursors.borrow().iter().next() {
            if !self.pos_in_edit_visible_range(first_cursor.row, first_cursor.col) {
                if first_cursor.row < self.row_offset {
                    self.row_offset = first_cursor.row;
                } else {
                    self.row_offset += first_cursor.row - (self.row_offset + (self.num_rows - 2))
                }
            }
        }
    }

    pub fn scroll_vertical(&mut self, delta: isize) {
        if let Some(result) = self.row_offset.checked_add_signed(delta) {
            self.row_offset = min(result, self.lines.borrow().len().saturating_sub(1));
        }
    }

    pub fn language(&self) -> Ref<'_, Language> {
        self.language.borrow()
    }

    fn pos_in_edit_visible_range(&self, row: usize, col: usize) -> bool {
        (self.row_offset..self.row_offset + self.num_rows.saturating_sub(1)).contains(&row)
            && (self.col_offset..self.col_offset + self.num_cols).contains(&col)
    }

    fn pos_in_render_visible_range(&self, row: usize, col: usize) -> bool {
        (self.row_offset..self.row_offset + self.num_rows).contains(&row)
            && (self.col_offset..self.col_offset + self.num_cols).contains(&col)
    }

    fn absolute_to_view_pos(&self, row: usize, col: usize) -> (usize, usize) {
        (
            row.saturating_sub(self.row_offset),
            col.saturating_sub(self.col_offset),
        )
    }
}
