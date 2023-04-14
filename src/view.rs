use std::cmp::{max, min};

use crate::{
    buffer::{Buffer, BufferMode},
    cursor::CompletionRequest,
    DeviceInput,
};

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
                let line = buffer.piece_table.line_index(cursor.position);
                let anchor_line = buffer.piece_table.line_index(cursor.anchor);
                for line in min(line, anchor_line)..=max(line, anchor_line) {
                    let start = 0;
                    let end = buffer.piece_table.line_at_index(line).unwrap().length;
                    let num = (start..=end)
                        .filter(|col| {
                            self.pos_in_render_visible_range(line, *col, num_rows, num_cols)
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
                for range in cursor.get_selection_ranges(&buffer.piece_table) {
                    let num = (range.start..=range.end)
                        .filter(|col| {
                            self.pos_in_render_visible_range(range.line, *col, num_rows, num_cols)
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
            let (line, col) = cursor.get_line_col(&buffer.piece_table);
            if self.pos_in_render_visible_range(line, col, num_rows, num_cols) {
                f(
                    self.absolute_to_view_row(line),
                    self.absolute_to_view_col(col),
                );
            }
        }
    }

    pub fn visible_completions<F>(&self, buffer: &Buffer, num_rows: usize, num_cols: usize, f: F)
    where
        F: Fn(usize, usize, CompletionRequest),
    {
        for cursor in buffer.cursors.iter() {
            if let Some((position, request)) = cursor.completion_context.get_last_request() {
                let line = buffer.piece_table.line_index(cursor.position);
                let col = buffer.piece_table.col_index(position);

                if self.pos_in_render_visible_range(line, col, num_rows, num_cols) {
                    f(
                        self.absolute_to_view_row(line),
                        self.absolute_to_view_col(col),
                        *request,
                    );
                }
            }
        }
    }

    pub fn visible_lines_iter<F>(&self, buffer: &Buffer, num_rows: usize, num_cols: usize, f: F)
    where
        F: Fn(usize, &[u8]),
    {
        buffer
            .piece_table
            .lines_foreach(self.line_offset, num_rows, |i, line| f(i, line));
    }

    // TODO: ASSUMES FIRST CURSOR IS FIRST
    pub fn adjust(&mut self, buffer: &Buffer, num_rows: usize, num_cols: usize) {
        if let Some(first_cursor) = buffer.cursors.first() {
            let (line, col) = first_cursor.get_line_col(&buffer.piece_table);
            if !self.pos_in_edit_visible_range(line, col, num_rows, num_cols) {
                if line < self.line_offset {
                    self.line_offset = line;
                } else if line > (self.line_offset + (num_rows - 2)) {
                    self.line_offset += line - (self.line_offset + (num_rows - 2))
                }

                if col < self.col_offset {
                    self.col_offset = col;
                } else if col > (self.col_offset + (num_cols - 2)) {
                    self.col_offset += col - (self.col_offset + (num_cols - 2))
                }
            }
        }
    }

    fn scroll_vertical(&mut self, buffer: &Buffer, delta: isize) {
        if let Some(result) = self.line_offset.checked_add_signed(delta) {
            self.line_offset = min(
                result,
                buffer
                    .piece_table
                    .iter_chars()
                    .filter(|c| *c == b'\n')
                    .count()
                    .saturating_sub(1),
            );
        }
    }

    fn pos_in_edit_visible_range(
        &self,
        line: usize,
        col: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> bool {
        (self.line_offset..self.line_offset + num_rows.saturating_sub(1)).contains(&line)
            && (self.col_offset..self.col_offset + num_cols.saturating_sub(1)).contains(&col)
    }

    fn pos_in_render_visible_range(
        &self,
        line: usize,
        col: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> bool {
        (self.line_offset..self.line_offset + num_rows).contains(&line)
            && (self.col_offset..self.col_offset + num_cols).contains(&col)
    }

    fn absolute_to_view_row(&self, line: usize) -> usize {
        line.saturating_sub(self.line_offset)
    }

    fn absolute_to_view_col(&self, col: usize) -> usize {
        col.saturating_sub(self.col_offset)
    }
}
