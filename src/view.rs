use std::cmp::{max, min};

use crate::buffer::{Buffer, DeviceInput};

const SCROLL_LINES_PER_ROLL: isize = 3;

pub struct View {
    pub row_offset: usize,
    pub col_offset: usize,
}

impl View {
    pub fn new() -> Self {
        Self {
            row_offset: 0,
            col_offset: 0,
        }
    }

    pub fn handle_input(&mut self, buffer: &Buffer, event: DeviceInput) {
        match event {
            DeviceInput::MouseWheel(sign) => {
                self.scroll_vertical(buffer, -sign * SCROLL_LINES_PER_ROLL)
            }
        }
    }

    pub fn visible_cursors_iter<F>(&self, buffer: &Buffer, num_rows: usize, num_cols: usize, f: F)
    where
        F: Fn(usize, usize, usize),
    {
        for cursor in buffer.cursors.iter() {
            if cursor.row == cursor.anchor_row {
                let first_col = min(cursor.col, cursor.anchor_col);
                let last_col = max(cursor.col, cursor.anchor_col);

                let num = (first_col..=last_col)
                    .filter(|col| {
                        self.pos_in_render_visible_range(cursor.row, *col, num_rows, num_cols)
                    })
                    .count();
                f(
                    self.absolute_to_view_row(cursor.row),
                    self.absolute_to_view_col(first_col),
                    num,
                );
            } else {
                let (first_row, first_col, last_row, last_col) = if cursor.row < cursor.anchor_row {
                    (cursor.row, cursor.col, cursor.anchor_row, cursor.anchor_col)
                } else {
                    (cursor.anchor_row, cursor.anchor_col, cursor.row, cursor.col)
                };

                let first_line_num = (first_col..=buffer.lines[first_row].len().saturating_sub(1))
                    .filter(|col| {
                        self.pos_in_render_visible_range(first_row, *col, num_rows, num_cols)
                    })
                    .count();
                f(
                    self.absolute_to_view_row(first_row),
                    self.absolute_to_view_col(first_col),
                    first_line_num,
                );

                for row in (first_row + 1)..last_row {
                    let num = (0..=buffer.lines[row].len().saturating_sub(1))
                        .filter(|col| {
                            self.pos_in_render_visible_range(row, *col, num_rows, num_cols)
                        })
                        .count();
                    f(
                        self.absolute_to_view_row(row),
                        self.absolute_to_view_col(0),
                        max(num, 1),
                    );
                }

                let last_line_num = (0..=last_col)
                    .filter(|col| {
                        self.pos_in_render_visible_range(last_row, *col, num_rows, num_cols)
                    })
                    .count();
                f(
                    self.absolute_to_view_row(last_row),
                    self.absolute_to_view_col(0),
                    last_line_num,
                );
            }
        }
    }

    pub fn visible_cursor_leads_iter<F>(
        &self,
        buffer: &Buffer,
        num_rows: usize,
        num_cols: usize,
        f: F,
    ) where
        F: Fn(usize, usize),
    {
        for cursor in buffer.cursors.iter() {
            if self.pos_in_render_visible_range(cursor.row, cursor.col, num_rows, num_cols) {
                f(
                    self.absolute_to_view_row(cursor.row),
                    self.absolute_to_view_col(cursor.col),
                );
            }
        }
    }

    pub fn visible_lines_iter<F>(&self, buffer: &Buffer, num_rows: usize, num_cols: usize, f: F)
    where
        F: Fn(usize, &[u8]),
    {
        for (i, line) in buffer.lines
            [self.row_offset..min(self.row_offset + num_rows, buffer.lines.len())]
            .iter()
            .enumerate()
        {
            f(i, line);
        }
    }

    pub fn adjust(&mut self, buffer: &Buffer, num_rows: usize, num_cols: usize) {
        if let Some(first_cursor) = buffer.cursors.first() {
            if !self.pos_in_edit_visible_range(
                first_cursor.row,
                first_cursor.col,
                num_rows,
                num_cols,
            ) {
                if first_cursor.row < self.row_offset {
                    self.row_offset = first_cursor.row;
                } else {
                    self.row_offset += first_cursor.row - (self.row_offset + (num_rows - 2))
                }
            }
        }
    }

    fn scroll_vertical(&mut self, buffer: &Buffer, delta: isize) {
        if let Some(result) = self.row_offset.checked_add_signed(delta) {
            self.row_offset = min(result, buffer.lines.len().saturating_sub(1));
        }
    }

    fn pos_in_edit_visible_range(
        &self,
        row: usize,
        col: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> bool {
        (self.row_offset..self.row_offset + num_rows.saturating_sub(1)).contains(&row)
            && (self.col_offset..self.col_offset + num_cols).contains(&col)
    }

    fn pos_in_render_visible_range(
        &self,
        row: usize,
        col: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> bool {
        (self.row_offset..self.row_offset + num_rows).contains(&row)
            && (self.col_offset..self.col_offset + num_cols).contains(&col)
    }

    fn absolute_to_view_row(&self, row: usize) -> usize {
        row.saturating_sub(self.row_offset)
    }

    fn absolute_to_view_col(&self, col: usize) -> usize {
        col.saturating_sub(self.col_offset)
    }
}
