use std::cmp::{max, min};

use crate::buffer::{Buffer, BufferMode, DeviceInput};

const SCROLL_LINES_PER_ROLL: isize = 3;

pub struct View {
    pub line_offset: usize,
    pub col_offset: usize,
}

impl View {
    pub fn new() -> Self {
        Self {
            line_offset: 0,
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
        if buffer.mode == BufferMode::VisualLine {
            for cursor in buffer.cursors.iter() {
                for line in
                    min(cursor.line, cursor.anchor_line)..=max(cursor.line, cursor.anchor_line)
                {
                    let start = 0;
                    let end = buffer.lines[line].len().saturating_sub(1);
                    let num = (start..=end)
                        .filter(|col| {
                            self.pos_in_render_visible_range(cursor.line, *col, num_rows, num_cols)
                        })
                        .count();
                    f(
                        self.absolute_to_view_row(line),
                        self.absolute_to_view_col(start),
                        num,
                    );
                }
            }
        } else {
            for cursor in buffer.cursors.iter() {
                for range in cursor.get_selection_ranges(&buffer.lines) {
                    let num = (range.start..=range.end)
                        .filter(|line| {
                            self.pos_in_render_visible_range(cursor.line, *line, num_rows, num_cols)
                        })
                        .count();
                    f(
                        self.absolute_to_view_row(range.line),
                        self.absolute_to_view_col(range.start),
                        num,
                    );
                }
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
            if self.pos_in_render_visible_range(cursor.line, cursor.col, num_rows, num_cols) {
                f(
                    self.absolute_to_view_row(cursor.line),
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
            [self.line_offset..min(self.line_offset + num_rows, buffer.lines.len())]
            .iter()
            .enumerate()
        {
            f(i, line);
        }
    }

    pub fn adjust(&mut self, buffer: &Buffer, num_rows: usize, num_cols: usize) {
        if let Some(first_cursor) = buffer.cursors.first() {
            if !self.pos_in_edit_visible_range(
                first_cursor.line,
                first_cursor.col,
                num_rows,
                num_cols,
            ) {
                if first_cursor.line < self.line_offset {
                    self.line_offset = first_cursor.line;
                } else if first_cursor.line > (self.line_offset + (num_rows - 2)) {
                    self.line_offset += first_cursor.line - (self.line_offset + (num_rows - 2))
                }

                if first_cursor.col < self.col_offset {
                    self.col_offset = first_cursor.col;
                } else if first_cursor.col > (self.col_offset + (num_cols - 2)) {
                    self.col_offset += first_cursor.col - (self.col_offset + (num_cols - 2))
                }
            }
        }
    }

    fn scroll_vertical(&mut self, buffer: &Buffer, delta: isize) {
        if let Some(result) = self.line_offset.checked_add_signed(delta) {
            self.line_offset = min(result, buffer.lines.len().saturating_sub(1));
        }
    }

    fn pos_in_edit_visible_range(
        &self,
        row: usize,
        col: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> bool {
        (self.line_offset..self.line_offset + num_rows.saturating_sub(1)).contains(&row)
            && (self.col_offset..self.col_offset + num_cols.saturating_sub(1)).contains(&col)
    }

    fn pos_in_render_visible_range(
        &self,
        row: usize,
        line: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> bool {
        (self.line_offset..self.line_offset + num_rows).contains(&row)
            && (self.col_offset..self.col_offset + num_cols).contains(&line)
    }

    fn absolute_to_view_row(&self, line: usize) -> usize {
        line.saturating_sub(self.line_offset)
    }

    fn absolute_to_view_col(&self, col: usize) -> usize {
        col.saturating_sub(self.col_offset)
    }
}
