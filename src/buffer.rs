use std::{cmp::min, fs::File, io::BufReader, str::pattern::Pattern};

use bstr::io::BufReadExt;
use winit::event::VirtualKeyCode;

use crate::{cursor::Cursor, language_support::Language};

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
        match key_code {
            VirtualKeyCode::Escape => {
                self.switch_to_normal_mode();
            }
            _ => (),
        }

        self.cursors.sort_unstable();
        self.cursors.dedup();
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
                        self.motion(CursorMotion::ForwardToChar(s.chars().nth(1).unwrap() as u8));
                    }
                    s if s.starts_with("F") && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToChar(s.chars().nth(1).unwrap() as u8));
                    }

                    "x" => self.command(BufferCommand::CutSelection),
                    "dd" => self.command(BufferCommand::DeleteLine),
                    "J" => self.command(BufferCommand::InsertCursorBelow),
                    "K" => self.command(BufferCommand::InsertCursorAbove),
                    s if s.starts_with("r") && s.len() == 2 => {
                        self.command(BufferCommand::ReplaceChar(s.chars().nth(1).unwrap() as u8));
                    }

                    "v" => {
                        self.switch_to_visual_mode();
                        return;
                    }
                    "V" => {
                        self.switch_to_visual_line_mode();
                        return;
                    }
                    _ => return,
                }

                self.input.clear();
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
                        self.motion(CursorMotion::ForwardToChar(s.chars().nth(1).unwrap() as u8));
                    }
                    s if s.starts_with("F") && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToChar(s.chars().nth(1).unwrap() as u8));
                    }

                    "x" => self.command(BufferCommand::CutSelection),
                    "d" => self.command(BufferCommand::CutSelection),
                    _ => return,
                }

                self.input.clear();
            }
            _ => (),
        }

        self.cursors.sort_unstable();
        self.cursors.dedup();
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
                CursorMotion::ForwardToChar(c) => {
                    cursor.move_forward_to_char(&self.lines, c);
                    cursor.unstick_col();
                }
                CursorMotion::BackwardToChar(c) => {
                    cursor.move_backward_to_char(&self.lines, c);
                    cursor.unstick_col();
                }
            }
        }
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
                for (deleted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    self.lines.remove(cursor.row - deleted_lines);
                    cursor.row = min(
                        cursor.row - deleted_lines,
                        self.lines.len().saturating_sub(1),
                    );
                    cursor.move_to_first_non_blank_char(&self.lines);
                }
            }
        }
    }

    fn switch_to_normal_mode(&mut self) {
        self.mode = BufferMode::Normal;
        self.input.clear();
        self.cursors.truncate(1);
        self.cursors[0].reset_anchor();
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
}
fn is_prefix_of_visual_command(str: &str) -> bool {
    VISUAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
        || (str.starts_with("f") && str.len() <= 2)
        || (str.starts_with("F") && str.len() <= 2)
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

pub enum DeviceInput {
    MouseWheel(isize),
}
