use std::cmp::{max, min};

use winit::dpi::LogicalPosition;

use crate::{
    buffer::{Buffer, BufferMode},
    cursor::CompletionRequest,
    language_server_types::{CompletionList, Diagnostic},
    piece_table::PieceTable,
};

const SCROLL_LINES_PER_ROLL: isize = 3;
const MAX_SHOWN_COMPLETION_ITEMS: usize = 10;

pub struct CompletionView {
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub height: usize,
}

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

    pub fn handle_scroll(&mut self, buffer: &Buffer, sign: isize) {
        self.scroll_vertical(buffer, -sign * SCROLL_LINES_PER_ROLL)
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
        F: Fn(&CompletionList, &CompletionView, &CompletionRequest),
    {
        if let Some(server) = &buffer.language_server {
            for cursor in buffer.cursors.iter() {
                if let Some(request) = cursor.completion_request {
                    if let Some(completion) = server.borrow().saved_completions.get(&request.id) {
                        if completion.items.is_empty() {
                            continue;
                        }

                        if let Some(completion_view) = self.get_completion_view(
                            &buffer.piece_table,
                            completion,
                            request.position,
                            num_rows,
                            num_cols,
                        ) {
                            f(completion, &completion_view, &request);
                        }
                    }
                }
            }
        }
    }

    pub fn visible_text(&self, buffer: &Buffer, num_rows: usize) -> Vec<u8> {
        buffer
            .piece_table
            .text_between_lines(self.line_offset, self.line_offset + num_rows)
    }

    pub fn visible_diagnostics_iter<F>(
        &self,
        buffer: &Buffer,
        diagnostics: &[Diagnostic],
        num_rows: usize,
        num_cols: usize,
        mut f: F,
    ) where
        F: FnMut(usize, (usize, usize), (usize, usize)),
    {
        if let Some(offset) = buffer
            .piece_table
            .char_index_from_line_col(self.line_offset, self.col_offset)
        {
            for diagnostic in diagnostics {
                if diagnostic.severity.is_some_and(|s| s > 2) {
                    continue;
                }

                let (start_row, start_col) = (
                    diagnostic.range.start.line as usize,
                    diagnostic.range.start.character as usize,
                );
                let (end_row, end_col) = (
                    diagnostic.range.end.line as usize,
                    diagnostic.range.end.character as usize,
                );

                if self.pos_in_render_visible_range(start_row, start_col, num_rows, num_cols)
                    || self.pos_in_render_visible_range(end_row, end_col, num_rows, num_cols)
                {
                    f(offset, (start_row, start_col), (end_row, end_col));
                }
            }
        }
    }

    pub fn adjust(&mut self, buffer: &Buffer, num_rows: usize, num_cols: usize) {
        if let Some(last_cursor) = buffer.cursors.last() {
            let (line, col) = last_cursor.get_line_col(&buffer.piece_table);
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

    pub fn get_completion_view(
        &self,
        piece_table: &PieceTable,
        completion: &CompletionList,
        position: usize,
        num_rows: usize,
        num_cols: usize,
    ) -> Option<CompletionView> {
        let line = piece_table.line_index(position);
        let col = piece_table.col_index(position);

        if !self.pos_in_render_visible_range(line, col, num_rows, num_cols) {
            return None;
        }

        let longest_string = completion
            .items
            .iter()
            .max_by(|x, y| {
                x.insert_text
                    .as_ref()
                    .unwrap_or(&x.label)
                    .len()
                    .cmp(&y.insert_text.as_ref().unwrap_or(&y.label).len())
            })
            .map(|x| x.insert_text.as_ref().unwrap_or(&x.label).len() + 1)
            .unwrap_or(0);

        let mut num_shown_completion_items =
            min(MAX_SHOWN_COMPLETION_ITEMS, completion.items.len());

        let row = self.absolute_to_view_row(line);
        let col = self.absolute_to_view_col(col);

        let available_rows_above = row.saturating_sub(1);
        let available_rows_below = num_rows.saturating_sub(row + 2);

        let grow_up = available_rows_below < 5 && available_rows_above > available_rows_below;
        let row = if grow_up {
            num_shown_completion_items = min(num_shown_completion_items, available_rows_above);
            row.saturating_sub(num_shown_completion_items)
        } else {
            num_shown_completion_items = min(num_shown_completion_items, available_rows_below);
            row + 1
        };

        let available_rows_right = num_cols.saturating_sub(col + 1);
        let move_left = available_rows_right < longest_string;
        let col = if move_left {
            col.saturating_sub(longest_string)
        } else {
            col
        };

        Some(CompletionView {
            row,
            col,
            width: longest_string,
            height: num_shown_completion_items,
        })
    }

    pub fn get_line_col(
        &self,
        mouse_position: LogicalPosition<f64>,
        font_size: (f64, f64),
    ) -> (usize, usize) {
        let row = (mouse_position.y / font_size.1 as f64).floor() as usize;
        let col = (mouse_position.x / font_size.0 as f64).floor() as usize;
        (row + self.line_offset, col + self.col_offset)
    }

    fn absolute_to_view_row(&self, line: usize) -> usize {
        line.saturating_sub(self.line_offset)
    }

    fn absolute_to_view_col(&self, col: usize) -> usize {
        col.saturating_sub(self.col_offset)
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
}
