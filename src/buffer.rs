use std::{cmp::min, fs::File, io::BufReader};

use bstr::io::BufReadExt;

use crate::{cursor::Cursor, language_support::Language};

pub enum CursorMotion {
    Forward(usize),
    Backward(usize),
    Up(usize),
    Down(usize),
    ForwardByWord,
    BackwardByWord,
    ToStartOfLine,
    ToEndOfLine,
    ToStartOfFile,
    ToEndOfFile,
    ForwardToChar(u8),
    BackwardToChar(u8),
}

pub enum BufferCommand {
    InsertCursorAbove,
    InsertCursorBelow,
    CutSelection,
    ReplaceChar(u8),
    DeleteLine,
}

struct View {
    row_offset: usize,
    col_offset: usize,
    num_rows: usize,
    num_cols: usize,
}

impl View {
    pub fn new(num_rows: usize, num_cols: usize) -> Self {
        Self {
            row_offset: 0,
            col_offset: 0,
            num_rows,
            num_cols,
        }
    }
}

pub struct Buffer {
    pub path: String,
    pub language: Language,
    view: View,
    lines: Vec<Vec<u8>>,
    cursors: Vec<Cursor>,
}

impl Buffer {
    pub fn new(path: &str, num_rows: usize, num_cols: usize) -> Self {
        Self {
            path: path.to_string(),
            language: Language::new(path),
            view: View::new(num_rows, num_cols),
            lines: BufReader::new(File::open(path).unwrap())
                .byte_lines()
                .try_collect()
                .unwrap(),
            cursors: vec![Cursor::new(0, 0)],
        }
    }

    pub fn motion(&mut self, motion: CursorMotion) {
        for cursor in &mut self.cursors {
            match motion {
                CursorMotion::Forward(count) => {
                    cursor.move_forward(&self.lines, count);
                }
                CursorMotion::Backward(count) => {
                    cursor.move_backward(count);
                }
                CursorMotion::Up(count) => {
                    cursor.move_up(&self.lines, count);
                }
                CursorMotion::Down(count) => {
                    cursor.move_down(&self.lines, count);
                }
                CursorMotion::ForwardByWord => {
                    cursor.move_forward_by_word(&self.lines);
                }
                CursorMotion::BackwardByWord => {
                    cursor.move_backward_by_word(&self.lines);
                }
                CursorMotion::ToStartOfLine => {
                    cursor.move_to_start_of_line();
                }
                CursorMotion::ToEndOfLine => {
                    cursor.move_to_end_of_line(&self.lines);
                }
                CursorMotion::ToStartOfFile => {
                    cursor.move_to_start_of_file();
                }
                CursorMotion::ToEndOfFile => {
                    cursor.move_to_end_of_file(&self.lines);
                }
                CursorMotion::ForwardToChar(c) => {
                    cursor.move_forward_to_char(&self.lines, c);
                }
                CursorMotion::BackwardToChar(c) => {
                    cursor.move_backward_to_char(&self.lines, c);
                }
            }
        }

        self.cursors.dedup();
    }

    pub fn command(&mut self, command: BufferCommand) {
        match command {
            BufferCommand::InsertCursorAbove => {
                if let Some(first_cursor) = self.cursors.first() {
                    if first_cursor.row == 0 {
                        return;
                    }

                    let row_above = first_cursor.row - 1;

                    self.cursors.push(Cursor::new(
                        row_above,
                        min(
                            first_cursor.col,
                            self.lines[row_above].len().saturating_sub(1),
                        ),
                    ));
                }
            }
            BufferCommand::InsertCursorBelow => {
                if let Some(last_cursor) = self.cursors.last() {
                    if last_cursor.row == self.lines.len().saturating_sub(1) {
                        return;
                    }

                    let row_below = last_cursor.row + 1;

                    self.cursors.push(Cursor::new(
                        row_below,
                        min(
                            last_cursor.col,
                            self.lines[row_below].len().saturating_sub(1),
                        ),
                    ));
                }
            }
            BufferCommand::CutSelection => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.row].is_empty() {
                        self.lines[cursor.row].remove(cursor.col);
                        cursor.col =
                            min(cursor.col, self.lines[cursor.row].len().saturating_sub(1));
                    }
                }
            }
            BufferCommand::ReplaceChar(c) => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.row].is_empty() {
                        self.lines[cursor.row][cursor.col] = c;
                    }
                }
            }
            BufferCommand::DeleteLine => {
                let mut deleted_lines = 0;
                for cursor in &mut self.cursors {
                    self.lines.remove(cursor.row - deleted_lines);
                    cursor.row = min(
                        cursor.row - deleted_lines,
                        self.lines.len().saturating_sub(1),
                    );
                    deleted_lines += 1;
                }
            }
        }

        self.cursors.sort_unstable();
        self.cursors.dedup();
    }

    pub fn visible_cursors_iter<F>(&self, f: F)
    where
        F: Fn(usize, usize),
    {
        for cursor in self.cursors.iter() {
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
        for (i, line) in self.lines
            [self.view.row_offset..min(self.view.row_offset + self.view.num_rows, self.lines.len())]
            .iter()
            .enumerate()
        {
            f(i, line);
        }
    }

    pub fn adjust_view(&mut self) {
        if let Some(first_cursor) = self.cursors.first() {
            if !self.pos_in_edit_visible_range(first_cursor.row, first_cursor.col) {
                if first_cursor.row < self.view.row_offset {
                    self.view.row_offset = first_cursor.row;
                } else {
                    self.view.row_offset +=
                        first_cursor.row - (self.view.row_offset + (self.view.num_rows - 2))
                }
            }
        }
    }

    pub fn scroll_vertical(&mut self, delta: isize) {
        if let Some(result) = self.view.row_offset.checked_add_signed(delta) {
            self.view.row_offset = min(result, self.lines.len().saturating_sub(1));
        }
    }

    fn pos_in_edit_visible_range(&self, row: usize, col: usize) -> bool {
        (self.view.row_offset..self.view.row_offset + self.view.num_rows.saturating_sub(1))
            .contains(&row)
            && (self.view.col_offset..self.view.col_offset + self.view.num_cols).contains(&col)
    }

    fn pos_in_render_visible_range(&self, row: usize, col: usize) -> bool {
        (self.view.row_offset..self.view.row_offset + self.view.num_rows).contains(&row)
            && (self.view.col_offset..self.view.col_offset + self.view.num_cols).contains(&col)
    }

    fn absolute_to_view_pos(&self, row: usize, col: usize) -> (usize, usize) {
        (
            row.saturating_sub(self.view.row_offset),
            col.saturating_sub(self.view.col_offset),
        )
    }
}
