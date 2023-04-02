use std::{
    cmp::{max, min},
    collections::HashMap,
    fs::File,
    io::BufReader,
    str::pattern::Pattern,
};

use bstr::{io::BufReadExt, ByteVec};
use winit::event::VirtualKeyCode;

use crate::{
    cursor::{cursors_overlapping, Cursor},
    language_support::Language,
};

pub struct Buffer {
    pub path: String,
    pub language: Language,
    pub lines: Vec<Vec<u8>>,
    pub cursors: Vec<Cursor>,
    pub mode: BufferMode,
    input: String,
}

impl Buffer {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            language: Language::new(path),
            lines: BufReader::new(File::open(path).unwrap())
                .byte_lines()
                .try_collect()
                .unwrap(),
            cursors: vec![Cursor::new(0, 0)],
            mode: BufferMode::Normal,
            input: String::new(),
        }
    }

    pub fn handle_key(&mut self, key_code: VirtualKeyCode) {
        match self.mode {
            BufferMode::Normal => {
                match key_code {
                    VirtualKeyCode::Escape => self.cursors.truncate(1),
                    VirtualKeyCode::Back => self.motion(CursorMotion::Backward(1)),
                    VirtualKeyCode::Delete => {
                        self.normal_command(NormalBufferCommand::CutSelection)
                    }
                    VirtualKeyCode::Return => self.motion(CursorMotion::Down(1)),
                    _ => (),
                }
                for cursor in &mut self.cursors {
                    cursor.reset_anchor();
                }
            }
            BufferMode::Insert => {
                match key_code {
                    VirtualKeyCode::Escape => self.switch_to_normal_mode(),
                    VirtualKeyCode::Back => {
                        self.insert_command(InsertBufferCommand::DeleteCharBack)
                    }
                    VirtualKeyCode::Delete => {
                        self.insert_command(InsertBufferCommand::DeleteCharFront)
                    }
                    VirtualKeyCode::Return => {
                        self.insert_command(InsertBufferCommand::InsertLineBreak)
                    }
                    _ => (),
                }
                for cursor in &mut self.cursors {
                    cursor.reset_anchor();
                }
            }
            BufferMode::Visual => match key_code {
                VirtualKeyCode::Escape => self.switch_to_normal_mode(),
                VirtualKeyCode::Back => self.motion(CursorMotion::Backward(1)),
                VirtualKeyCode::Delete => self.visual_command(VisualBufferCommand::CutSelection),
                VirtualKeyCode::Return => self.motion(CursorMotion::Down(1)),
                _ => (),
            },
            BufferMode::VisualLine => match key_code {
                VirtualKeyCode::Escape => self.switch_to_normal_mode(),
                VirtualKeyCode::Back => self.motion(CursorMotion::Backward(1)),
                VirtualKeyCode::Delete => {
                    self.visual_line_command(VisualLineBufferCommand::CutSelection)
                }
                VirtualKeyCode::Return => self.motion(CursorMotion::Down(1)),
                _ => (),
            },
        }

        self.cursors.sort_unstable();
        self.cursors.dedup();

        // In insert mode, merging is impliclitly done by dedup.
        if self.mode != BufferMode::Insert {
            self.merge_cursors();
        }
    }

    pub fn handle_char(&mut self, c: char) {
        match self.mode {
            BufferMode::Normal => {
                self.input.push(c);

                if !is_prefix_of_normal_command(&self.input) {
                    self.input.clear();
                    self.input.push(c);
                }

                match self.input.as_str() {
                    "j" => self.motion(CursorMotion::Down(1)),
                    "k" => self.motion(CursorMotion::Up(1)),
                    "h" => self.motion(CursorMotion::Backward(1)),
                    "l" => self.motion(CursorMotion::Forward(1)),
                    "w" => self.motion(CursorMotion::ForwardByWord),
                    "b" => self.motion(CursorMotion::BackwardByWord),
                    "0" => self.motion(CursorMotion::ToStartOfLine),
                    "$" => self.motion(CursorMotion::ToEndOfLine),
                    "^" => self.motion(CursorMotion::ToFirstNonBlankChar),
                    "gg" => self.motion(CursorMotion::ToStartOfFile),
                    "G" => self.motion(CursorMotion::ToEndOfFile),
                    s if s.starts_with("f") && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with("F") && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with("t") && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with("T") && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }

                    "x" => self.normal_command(NormalBufferCommand::CutSelection),
                    "dd" => self.normal_command(NormalBufferCommand::DeleteLine),
                    "J" => self.normal_command(NormalBufferCommand::InsertCursorBelow),
                    "K" => self.normal_command(NormalBufferCommand::InsertCursorAbove),
                    s if s.starts_with("r") && s.len() == 2 => {
                        self.normal_command(NormalBufferCommand::ReplaceChar(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }

                    "i" => {
                        self.motion(CursorMotion::EnterInsertModeBeforeChar);
                        self.switch_to_insert_mode();
                    }
                    "I" => {
                        self.motion(CursorMotion::ToFirstNonBlankChar);
                        self.motion(CursorMotion::EnterInsertModeBeforeChar);
                        self.switch_to_insert_mode();
                    }
                    "a" => {
                        self.motion(CursorMotion::EnterInsertModeAfterChar);
                        self.switch_to_insert_mode();
                    }
                    "A" => {
                        self.motion(CursorMotion::ToEndOfLine);
                        self.motion(CursorMotion::EnterInsertModeAfterChar);
                        self.switch_to_insert_mode();
                    }
                    "o" => {
                        self.normal_command(NormalBufferCommand::InsertLineBelow);
                        self.motion(CursorMotion::Down(1));
                        self.motion(CursorMotion::ToStartOfLine);
                        self.motion(CursorMotion::EnterInsertModeBeforeChar);
                        self.switch_to_insert_mode();
                    }
                    "O" => {
                        self.normal_command(NormalBufferCommand::InsertLineAbove);
                        self.motion(CursorMotion::ToStartOfLine);
                        self.motion(CursorMotion::EnterInsertModeBeforeChar);
                        self.switch_to_insert_mode();
                    }
                    "v" => {
                        self.switch_to_visual_mode();
                    }
                    "V" => {
                        self.switch_to_visual_line_mode();
                    }
                    _ => return,
                }

                self.input.clear();
                for cursor in &mut self.cursors {
                    cursor.reset_anchor();
                }
            }
            BufferMode::Insert => {
                if c as u8 >= 0x20 && c as u8 <= 0x7E {
                    self.insert_command(InsertBufferCommand::InsertChar(c as u8));
                }
                for cursor in &mut self.cursors {
                    cursor.reset_anchor();
                }
            }
            BufferMode::Visual | BufferMode::VisualLine => {
                self.input.push(c);

                if !is_prefix_of_visual_command(&self.input) {
                    self.input.clear();
                    self.input.push(c);
                }

                match self.input.as_str() {
                    "j" => self.motion(CursorMotion::Down(1)),
                    "k" => self.motion(CursorMotion::Up(1)),
                    "h" => self.motion(CursorMotion::Backward(1)),
                    "l" => self.motion(CursorMotion::Forward(1)),
                    "w" => self.motion(CursorMotion::ForwardByWord),
                    "b" => self.motion(CursorMotion::BackwardByWord),
                    "0" => self.motion(CursorMotion::ToStartOfLine),
                    "$" => self.motion(CursorMotion::ToEndOfLine),
                    "^" => self.motion(CursorMotion::ToFirstNonBlankChar),
                    "gg" => self.motion(CursorMotion::ToStartOfFile),
                    "G" => self.motion(CursorMotion::ToEndOfFile),
                    s if s.starts_with("f") && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with("F") && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with("t") && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with("T") && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }

                    "x" => {
                        if self.mode == BufferMode::Visual {
                            self.visual_command(VisualBufferCommand::CutSelection);
                        } else {
                            self.visual_line_command(VisualLineBufferCommand::CutSelection)
                        }
                    }
                    "d" => {
                        if self.mode == BufferMode::Visual {
                            self.visual_command(VisualBufferCommand::CutSelection);
                        } else {
                            self.visual_line_command(VisualLineBufferCommand::CutSelection)
                        }
                    }

                    "v" => {
                        self.switch_to_visual_mode();
                    }
                    "V" => {
                        self.switch_to_visual_line_mode();
                    }
                    _ => return,
                }

                self.input.clear();
            }
        }

        self.cursors.sort_unstable();
        self.cursors.dedup();
        // In insert mode, merging is impliclitly done by dedup.
        if self.mode != BufferMode::Insert {
            self.merge_cursors();
        }
    }

    pub fn motion(&mut self, motion: CursorMotion) {
        for cursor in &mut self.cursors {
            match motion {
                CursorMotion::Forward(count) => {
                    cursor.move_forward(&self.lines, count);
                    cursor.unstick_col();
                }
                CursorMotion::Backward(count) => {
                    cursor.move_backward(&self.lines, count);
                    cursor.unstick_col();
                }
                CursorMotion::Up(count) => {
                    cursor.move_up(&self.lines, count);
                    cursor.stick_col();
                }
                CursorMotion::Down(count) => {
                    cursor.move_down(&self.lines, count);
                    cursor.stick_col();
                }
                CursorMotion::ForwardByWord => {
                    cursor.move_forward_by_word(&self.lines);
                    cursor.unstick_col();
                }
                CursorMotion::BackwardByWord => {
                    cursor.move_backward_by_word(&self.lines);
                    cursor.unstick_col();
                }
                CursorMotion::ToStartOfLine => {
                    cursor.move_to_start_of_line();
                    cursor.unstick_col();
                }
                CursorMotion::ToEndOfLine => {
                    cursor.move_to_end_of_line(&self.lines);
                    cursor.unstick_col();
                }
                CursorMotion::ToStartOfFile => {
                    cursor.move_to_start_of_file();
                    cursor.unstick_col();
                }
                CursorMotion::ToEndOfFile => {
                    cursor.move_to_end_of_file(&self.lines);
                    cursor.unstick_col();
                }
                CursorMotion::ToFirstNonBlankChar => {
                    cursor.move_to_first_non_blank_char(&self.lines);
                    cursor.unstick_col();
                }
                CursorMotion::ForwardToCharInclusive(c) => {
                    cursor.move_forward_to_char_inclusive(&self.lines, c);
                    cursor.unstick_col();
                }
                CursorMotion::BackwardToCharInclusive(c) => {
                    cursor.move_backward_to_char_inclusive(&self.lines, c);
                    cursor.unstick_col();
                }
                CursorMotion::ForwardToCharExclusive(c) => {
                    cursor.move_forward_to_char_exclusive(&self.lines, c);
                    cursor.unstick_col();
                }
                CursorMotion::BackwardToCharExclusive(c) => {
                    cursor.move_backward_to_char_exclusive(&self.lines, c);
                    cursor.unstick_col();
                }
                CursorMotion::EnterInsertModeBeforeChar => {
                    if cursor.col == 0 {
                        cursor.trailing = false;
                    } else {
                        cursor.move_backward(&self.lines, 1);
                        cursor.trailing = true;
                    }
                }
                CursorMotion::EnterInsertModeAfterChar => {
                    cursor.trailing = !self.lines[cursor.row].is_empty();
                }
            }
        }
    }

    pub fn normal_command(&mut self, command: NormalBufferCommand) {
        match command {
            NormalBufferCommand::InsertCursorAbove => {
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
            NormalBufferCommand::InsertCursorBelow => {
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
            NormalBufferCommand::CutSelection => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.row].is_empty() {
                        self.lines[cursor.row].remove(cursor.col);
                        cursor.col =
                            min(cursor.col, self.lines[cursor.row].len().saturating_sub(1));
                    }
                }
            }
            NormalBufferCommand::ReplaceChar(c) => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.row].is_empty() {
                        self.lines[cursor.row][cursor.col] = c;
                    }
                }
            }
            NormalBufferCommand::DeleteLine => {
                for (deleted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    // Special case: if only the last line remains, simply delete its content.
                    if cursor.row - deleted_lines == 0 && self.lines.len() == 1 {
                        self.lines[0].clear();
                    } else {
                        self.lines.remove(cursor.row - deleted_lines);
                    }

                    cursor.row = cursor.row.saturating_sub(deleted_lines);
                    cursor.move_to_first_non_blank_char(&self.lines);
                }
            }
            NormalBufferCommand::InsertLineAbove => {
                for (insterted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    cursor.row += insterted_lines;
                    self.lines.insert(cursor.row, vec![]);
                }
            }
            NormalBufferCommand::InsertLineBelow => {
                for (insterted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    cursor.row += insterted_lines;
                    self.lines.insert(cursor.row + 1, vec![]);
                }
            }
        }
    }
    pub fn visual_command(&mut self, command: VisualBufferCommand) {
        match command {
            VisualBufferCommand::CutSelection => {
                let mut lines_to_delete = vec![];
                let mut deleted_lines = 0;
                for cursor in self.cursors.iter_mut() {
                    let selection_ranges = cursor.get_selection_ranges(&self.lines);
                    if selection_ranges.len() == 1 {
                        if let Some(range) = selection_ranges.first() {
                            if !self.lines[range.row].is_empty() {
                                self.lines[range.row].drain(range.start..=range.end);

                                let col = if range.start
                                    > self.lines[range.row].len().saturating_sub(1)
                                {
                                    range.start.saturating_sub(1)
                                } else {
                                    range.start
                                };
                                cursor.col = col;
                                cursor.anchor_col = col;
                                cursor.row = cursor.row.saturating_sub(deleted_lines);
                            }
                        }
                    }
                    if selection_ranges.len() > 1 {
                        if let (Some(first), Some(last)) =
                            (selection_ranges.first(), selection_ranges.last())
                        {
                            self.lines[first.row].drain(first.start..);
                            let end = Vec::from(
                                &self.lines[last.row]
                                    [min(last.end + 1, self.lines[last.row].len())..],
                            );
                            self.lines[first.row].push_str(&end);

                            let col = if first.start > self.lines[first.row].len().saturating_sub(1)
                            {
                                first.start.saturating_sub(1)
                            } else {
                                first.start
                            };
                            cursor.col = col;
                            cursor.anchor_col = col;
                            cursor.row =
                                min(cursor.row, cursor.anchor_row).saturating_sub(deleted_lines);
                            cursor.anchor_row = cursor.row;
                        }

                        for range in selection_ranges[1..].iter() {
                            lines_to_delete.push(range.row);
                            deleted_lines += 1;
                        }
                    }
                }

                lines_to_delete.sort();
                lines_to_delete.dedup();
                for line in lines_to_delete.iter().rev() {
                    self.lines.remove(*line);
                }

                self.switch_to_normal_mode();
            }
        }
    }

    pub fn visual_line_command(&mut self, command: VisualLineBufferCommand) {
        match command {
            VisualLineBufferCommand::CutSelection => {
                let mut lines_to_delete = vec![];
                let mut deleted_lines = 0;
                for cursor in self.cursors.iter_mut() {
                    let first_row = min(cursor.row, cursor.anchor_row);
                    let last_row = max(cursor.row, cursor.anchor_row);
                    cursor.row = first_row.saturating_sub(deleted_lines);
                    cursor.anchor_row = cursor.row;
                    cursor.col = 0;
                    cursor.anchor_col = 0;

                    for row in (first_row..=last_row).rev() {
                        lines_to_delete.push(row);
                        deleted_lines += 1;
                    }
                }

                lines_to_delete.sort();
                lines_to_delete.dedup();
                for line in lines_to_delete.iter().rev() {
                    self.lines.remove(*line);
                }

                // Special case, if all lines deleted insert an empty line.
                if self.lines.is_empty() {
                    self.lines.push(vec![]);
                }

                self.switch_to_normal_mode();
            }
        }
    }

    pub fn insert_command(&mut self, command: InsertBufferCommand) {
        match command {
            InsertBufferCommand::InsertChar(c) => {
                for cursor in &mut self.cursors {
                    self.lines[cursor.row].insert(cursor.col + cursor.trailing as usize, c);
                    if cursor.trailing {
                        cursor.col += 1;
                    } else {
                        cursor.trailing = true;
                    }
                }
            }
            InsertBufferCommand::DeleteCharBack => {
                let mut deleted_lines = 0;
                let mut deleted_cols = HashMap::<usize, usize>::new();
                for cursor in &mut self.cursors {
                    cursor.row -= deleted_lines;
                    if let Some(deleted_cols) = deleted_cols.get(&cursor.row) {
                        cursor.col -= deleted_cols;
                    }

                    if cursor.col == 0 && !cursor.trailing {
                        if cursor.row == 0 {
                            continue;
                        }
                        let end = self.lines[cursor.row].clone();
                        cursor.col = self.lines[cursor.row - 1].len().saturating_sub(1);
                        cursor.trailing = !self.lines[cursor.row - 1].is_empty();
                        self.lines[cursor.row - 1].push_str(&end);
                        self.lines.remove(cursor.row);
                        cursor.row -= 1;
                        deleted_lines += 1;
                    } else {
                        self.lines[cursor.row].remove(cursor.col);
                        match deleted_cols.get_mut(&cursor.row) {
                            None => {
                                deleted_cols.insert(cursor.row, 1);
                            }
                            Some(cols) => *cols += 1,
                        }
                        if cursor.col == 0 {
                            cursor.trailing = false;
                        } else {
                            cursor.col -= 1;
                        }
                    }
                }
            }
            InsertBufferCommand::DeleteCharFront => {
                let mut deleted_lines = 0;
                let mut column_offsets = HashMap::<usize, isize>::new();
                for cursor in &mut self.cursors {
                    cursor.row -= deleted_lines;
                    let mut row_offset = 0;
                    if let Some(offset) = column_offsets.get(&cursor.row) {
                        cursor.col = cursor.col.saturating_add_signed(*offset);
                        row_offset = *offset;
                    }

                    let line_length = cursor.line_zero_indexed_length(&self.lines);
                    if (cursor.col == line_length && cursor.trailing) || line_length == 0 {
                        if cursor.row == self.lines.len().saturating_sub(1) {
                            continue;
                        }
                        let end = self.lines[cursor.row + 1].clone();
                        self.lines[cursor.row].push_str(&end);
                        self.lines.remove(cursor.row + 1);
                        deleted_lines += 1;
                        row_offset -= (line_length + 1) as isize;
                        match column_offsets.get_mut(&cursor.row) {
                            None => {
                                column_offsets.insert(cursor.row, row_offset as isize);
                            }
                            Some(cols) => *cols = row_offset as isize,
                        }
                    } else {
                        self.lines[cursor.row].remove(cursor.col + 1);
                        match column_offsets.get_mut(&cursor.row) {
                            None => {
                                column_offsets.insert(cursor.row, -1);
                            }
                            Some(cols) => *cols -= 1,
                        }
                    }
                }
            }
            InsertBufferCommand::InsertLineBreak => {
                let mut inserted_lines = 0;
                let mut column_offsets = HashMap::<usize, isize>::new();
                for cursor in &mut self.cursors {
                    cursor.row += inserted_lines;
                    let mut row_offset = 0;
                    if let Some(offset) = column_offsets.get(&cursor.row) {
                        cursor.col = cursor.col.saturating_add_signed(*offset);
                        row_offset = *offset;
                    }

                    let end =
                        Vec::from(&self.lines[cursor.row][cursor.col + cursor.trailing as usize..]);
                    self.lines[cursor.row].drain(cursor.col + cursor.trailing as usize..);
                    row_offset -= self.lines[cursor.row].len() as isize;
                    self.lines.insert(cursor.row + 1, end);
                    cursor.row += 1;
                    cursor.col = 0;
                    cursor.trailing = false;
                    inserted_lines += 1;
                    match column_offsets.get_mut(&cursor.row) {
                        None => {
                            column_offsets.insert(cursor.row, row_offset as isize);
                        }
                        Some(cols) => *cols = row_offset as isize,
                    }
                }
            }
        }
    }

    fn merge_cursors(&mut self) {
        let mut merged = vec![];
        let mut current_cursor = self.cursors.first().unwrap().clone();

        // Since we are always moving all cursors at once, cursors can only merge in the "same direction"
        for cursor in &self.cursors[1..] {
            if cursors_overlapping(&current_cursor, cursor) {
                if cursor.moving_forward() {
                    current_cursor.row = cursor.row;
                    current_cursor.col = cursor.col;
                } else {
                    current_cursor.anchor_row = cursor.anchor_row;
                    current_cursor.anchor_col = cursor.anchor_col;
                }
            } else {
                merged.push(current_cursor);
                current_cursor = *cursor;
            }
        }
        merged.push(current_cursor);

        self.cursors = merged;
    }

    fn switch_to_normal_mode(&mut self) {
        self.mode = BufferMode::Normal;
        self.input.clear();
        for cursor in &mut self.cursors {
            cursor.reset_anchor();
        }
    }

    fn switch_to_insert_mode(&mut self) {
        self.mode = BufferMode::Insert;
        for cursor in &mut self.cursors {
            cursor.reset_anchor();
        }
    }

    fn switch_to_visual_mode(&mut self) {
        self.mode = BufferMode::Visual;
        self.input.clear();
    }

    fn switch_to_visual_line_mode(&mut self) {
        self.mode = BufferMode::VisualLine;
        self.input.clear();
    }
}

fn is_prefix_of_normal_command(str: &str) -> bool {
    NORMAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
        || (str.starts_with("f") && str.len() <= 2)
        || (str.starts_with("F") && str.len() <= 2)
        || (str.starts_with("r") && str.len() <= 2)
        || (str.starts_with("t") && str.len() <= 2)
        || (str.starts_with("T") && str.len() <= 2)
}
fn is_prefix_of_visual_command(str: &str) -> bool {
    VISUAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
        || (str.starts_with("f") && str.len() <= 2)
        || (str.starts_with("F") && str.len() <= 2)
        || (str.starts_with("t") && str.len() <= 2)
        || (str.starts_with("T") && str.len() <= 2)
}

const NORMAL_MODE_COMMANDS: [&str; 16] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "dd", "J", "K", "v", "V",
];
const VISUAL_MODE_COMMANDS: [&str; 12] =
    ["j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "d"];

#[derive(PartialEq)]
pub enum BufferMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

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
    ToFirstNonBlankChar,
    ForwardToCharInclusive(u8),
    BackwardToCharInclusive(u8),
    ForwardToCharExclusive(u8),
    BackwardToCharExclusive(u8),
    EnterInsertModeBeforeChar,
    EnterInsertModeAfterChar,
}

pub enum VisualBufferCommand {
    CutSelection,
}

pub enum VisualLineBufferCommand {
    CutSelection,
}

pub enum NormalBufferCommand {
    InsertCursorAbove,
    InsertCursorBelow,
    ReplaceChar(u8),
    CutSelection,
    DeleteLine,
    InsertLineAbove,
    InsertLineBelow,
}

pub enum InsertBufferCommand {
    InsertChar(u8),
    DeleteCharBack,
    DeleteCharFront,
    InsertLineBreak,
}

pub enum DeviceInput {
    MouseWheel(isize),
}
