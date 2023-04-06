use std::{
    cell::RefCell,
    cmp::{max, min},
    fs::File,
    io::BufReader,
    rc::Rc,
    str::pattern::Pattern,
};

use bstr::{io::BufReadExt, ByteVec};
use winit::event::VirtualKeyCode;

use crate::{
    cursor::{cursors_foreach_rebalance, cursors_overlapping, Cursor},
    language_server::LanguageServer,
    language_server_types::{DidOpenTextDocumentParams, TextDocumentItem},
    language_support::{language_from_path, Language},
};

pub struct Buffer {
    pub path: String,
    pub language: &'static Language,
    pub lines: Vec<Vec<u8>>,
    pub cursors: Vec<Cursor>,
    pub mode: BufferMode,
    language_server: Option<Rc<RefCell<LanguageServer>>>,
    input: String,
    version: i32,
}

impl Buffer {
    // TODO: Error handling
    pub fn new(path: &str, language_server: Option<Rc<RefCell<LanguageServer>>>) -> Self {
        let lines: Vec<Vec<u8>> = BufReader::new(File::open(path).unwrap())
            .byte_lines()
            .try_collect()
            .unwrap();
        let language = language_from_path(path).unwrap();
        let text = lines.join(&b'\n');

        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: "file:///".to_string() + path,
                language_id: language.identifier.to_string(),
                version: 0,
                text: unsafe { String::from_utf8_unchecked(text) },
            },
        };
        if let Some(server) = &language_server {
            server
                .borrow_mut()
                .send_notification("textDocument/didOpen", open_params);
        }

        Self {
            path: path.to_string(),
            language,
            lines,
            cursors: vec![Cursor::new(0, 0)],
            mode: BufferMode::Normal,
            language_server,
            input: String::new(),
            version: 1,
        }
    }

    pub fn handle_key(&mut self, key_code: VirtualKeyCode) {
        use BufferCommand::*;
        use BufferMode::*;
        use CursorMotion::*;
        use VirtualKeyCode::{Back, Delete, Escape, Return, Tab};

        match (self.mode, key_code) {
            (Normal, Escape) => self.cursors.truncate(1),
            (Insert, Escape) => {
                self.motion(Backward(1));
                self.switch_to_normal_mode();
            }
            (_, Escape) => self.switch_to_normal_mode(),

            (Insert, Back) => self.command(DeleteCharBack),
            (_, Back) => self.motion(Backward(1)),

            (Insert, Return) => self.command(InsertLineBreak),
            (_, Return) => self.motion(Down(1)),

            (Normal, Delete) => self.command(CutChar),
            (Visual, Delete) => {
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, Delete) => {
                self.command(CutLineSelection);
                self.switch_to_normal_mode();
            }
            (Insert, Delete) => self.command(DeleteCharFront),
            (Insert, Tab) => {
                for _ in 0..SPACES_PER_TAB {
                    self.command(InsertChar(b' '));
                }
            }
            _ => (),
        }

        self.cursors.sort_unstable();
        self.cursors.dedup();

        // In insert mode, merging is impliclitly done by dedup.
        if self.mode != Insert {
            self.merge_cursors();
        }
    }

    pub fn handle_char(&mut self, c: char) {
        use BufferCommand::*;
        use BufferMode::*;
        use CursorMotion::*;

        if self.mode == Insert {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                self.command(InsertChar(c as u8));
            }
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
            self.cursors.sort_unstable();
            self.cursors.dedup();
            return;
        }

        self.input.push(c);

        if !is_prefix_of_command(&self.input, self.mode) {
            self.input.clear();
            self.input.push(c);
        }

        match (self.mode, self.input.as_str()) {
            (_, "j") => self.motion(Down(1)),
            (_, "k") => self.motion(Up(1)),
            (_, "h") => self.motion(Backward(1)),
            (_, "l") => self.motion(Forward(1)),
            (_, "w") => self.motion(ForwardByWord),
            (_, "b") => self.motion(BackwardByWord),
            (_, "0") => self.motion(ToStartOfLine),
            (_, "$") => self.motion(ToEndOfLine),
            (_, "^") => self.motion(ToFirstNonBlankChar),
            (_, "gg") => self.motion(ToStartOfFile),
            (_, "G") => self.motion(ToEndOfFile),
            (_, s) if s.starts_with('f') && s.len() == 2 => {
                self.motion(ForwardToCharInclusive(s.chars().nth(1).unwrap() as u8));
            }
            (_, s) if s.starts_with('F') && s.len() == 2 => {
                self.motion(BackwardToCharInclusive(s.chars().nth(1).unwrap() as u8));
            }
            (_, s) if s.starts_with('t') && s.len() == 2 => {
                self.motion(ForwardToCharExclusive(s.chars().nth(1).unwrap() as u8));
            }
            (_, s) if s.starts_with('T') && s.len() == 2 => {
                self.motion(BackwardToCharExclusive(s.chars().nth(1).unwrap() as u8));
            }

            (Normal, "x") => self.command(CutChar),
            (Visual, "x") => self.command(CutSelection),
            (VisualLine, "x") => self.command(CutLineSelection),

            (Visual, "d") => {
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, "d") => {
                self.command(CutLineSelection);
                self.switch_to_normal_mode();
            }

            (Normal, "dd") => self.command(DeleteLine),
            (Normal, "J") => self.command(InsertCursorBelow),
            (Normal, "K") => self.command(InsertCursorAbove),
            (Normal, s) if s.starts_with('r') && s.len() == 2 => {
                self.command(ReplaceChar(s.chars().nth(1).unwrap() as u8));
            }
            (Normal, "i") => self.switch_to_insert_mode(),
            (Normal, "I") => {
                self.motion(ToFirstNonBlankChar);
                self.switch_to_insert_mode();
            }
            (Normal, "a") => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.line].is_empty() {
                        cursor.col += 1;
                    }
                }
                self.switch_to_insert_mode();
            }
            (Normal, "A") => {
                self.switch_to_insert_mode();
                self.motion(ToEndOfLine);
                self.motion(Forward(1));
            }
            (Normal, "o") => {
                self.switch_to_insert_mode();
                self.motion(ToEndOfLine);
                self.motion(Forward(1));
                self.command(InsertLineBreak);
            }
            (Normal, "O") => {
                self.switch_to_insert_mode();
                self.motion(ToStartOfLine);
                self.command(InsertLineBreak);
                self.motion(Up(1));
            }
            (Visual, "v") => self.switch_to_normal_mode(),
            (_, "v") => self.switch_to_visual_mode(),
            (VisualLine, "V") => self.switch_to_normal_mode(),
            (_, "V") => self.switch_to_visual_line_mode(),
            _ => return,
        }

        if self.mode == Normal {
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
        }
        self.input.clear();

        self.cursors.sort_unstable();
        self.cursors.dedup();
        self.merge_cursors();
    }

    pub fn motion(&mut self, motion: CursorMotion) {
        use BufferMode::*;
        use CursorMotion::*;
        for cursor in &mut self.cursors {
            // Cache the column position of the cursor when moving up or down
            match motion {
                Up(_) | Down(_) => cursor.stick_col(),
                _ => cursor.unstick_col(),
            }

            match motion {
                Forward(count) => {
                    if self.mode == Insert
                        && count == 1
                        && !self.lines[cursor.line].is_empty()
                        && cursor.col == cursor.line_zero_indexed_length(&self.lines)
                    {
                        cursor.col += 1;
                    } else {
                        cursor.move_forward(&self.lines, count);
                    }
                }
                Backward(count) => cursor.move_backward(&self.lines, count),
                Up(count) => cursor.move_up(&self.lines, count),
                Down(count) => cursor.move_down(&self.lines, count),
                ForwardByWord => cursor.move_forward_by_word(&self.lines),
                BackwardByWord => cursor.move_backward_by_word(&self.lines),
                ForwardOnceWrapping => cursor.move_forward_once_wrapping(&self.lines),
                ToStartOfLine => cursor.move_to_start_of_line(),
                ToEndOfLine => cursor.move_to_end_of_line(&self.lines),
                ToStartOfFile => cursor.move_to_start_of_file(),
                ToEndOfFile => cursor.move_to_end_of_file(&self.lines),
                ToFirstNonBlankChar => cursor.move_to_first_non_blank_char(&self.lines),
                ForwardToCharInclusive(c) => cursor.move_forward_to_char_inc(&self.lines, c),
                BackwardToCharInclusive(c) => cursor.move_backward_to_char_inc(&self.lines, c),
                ForwardToCharExclusive(c) => cursor.move_forward_to_char_exc(&self.lines, c),
                BackwardToCharExclusive(c) => cursor.move_backward_to_char_exc(&self.lines, c),
                SelectLine => cursor.select_line(&self.lines),
            }
        }

        if self.mode == Insert || self.mode == Normal {
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
        }
    }

    pub fn command(&mut self, command: BufferCommand) {
        use BufferCommand::*;
        use BufferMode::*;
        use CursorMotion::*;
        match command {
            InsertCursorAbove => {
                if let Some(first_cursor) = self.cursors.first() {
                    if first_cursor.line == 0 {
                        return;
                    }

                    let line_above = first_cursor.line - 1;

                    self.cursors.push(Cursor::new(
                        line_above,
                        min(
                            first_cursor.col,
                            self.lines[line_above].len().saturating_sub(1),
                        ),
                    ));
                }
            }
            InsertCursorBelow => {
                if let Some(last_cursor) = self.cursors.last() {
                    if last_cursor.line == self.lines.len().saturating_sub(1) {
                        return;
                    }

                    let line_below = last_cursor.line + 1;

                    self.cursors.push(Cursor::new(
                        line_below,
                        min(
                            last_cursor.col,
                            self.lines[line_below].len().saturating_sub(1),
                        ),
                    ));
                }
            }
            ReplaceChar(c) => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.line].is_empty() {
                        self.lines[cursor.line][cursor.col] = c;
                    }
                }
            }
            CutChar => {
                self.motion(Forward(1));
                self.command(DeleteCharBack);
            }
            CutSelection => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    let selection_ranges = cursor.get_selection_ranges(&self.lines);
                    if selection_ranges.len() == 1 {
                        if let Some(range) = selection_ranges.first() {
                            if !self.lines[range.line].is_empty() {
                                self.lines[range.line].drain(range.start..=range.end);

                                cursor.col = range.start;
                                cursor.anchor_col = range.start;
                            }
                        }
                    }
                    if selection_ranges.len() > 1 {
                        if let (Some(first), Some(last)) =
                            (selection_ranges.first(), selection_ranges.last())
                        {
                            self.lines[first.line].drain(first.start..);
                            let end = Vec::from(
                                &self.lines[last.line]
                                    [min(last.end + 1, self.lines[last.line].len())..],
                            );
                            self.lines[first.line].push_str(&end);

                            cursor.col = first.start;
                            cursor.anchor_col = first.start;
                            cursor.line = min(cursor.line, cursor.anchor_line);
                            cursor.anchor_line = cursor.line;
                        }

                        for range in selection_ranges[1..].iter().rev() {
                            self.lines.remove(range.line);
                        }
                    }
                });
            }
            CutLineSelection => {
                cursors_foreach_rebalance(self.cursors.as_mut_slice(), |cursor| {
                    let first_line = min(cursor.line, cursor.anchor_line);
                    let last_line = max(cursor.line, cursor.anchor_line);
                    cursor.line = first_line;
                    cursor.anchor_line = cursor.line;
                    cursor.col = 0;
                    cursor.anchor_col = 0;

                    for line in (first_line..=last_line).rev() {
                        self.lines.remove(line);
                    }
                });

                // Special case, if all lines deleted insert an empty line.
                if self.lines.is_empty() {
                    self.lines.push(vec![]);
                }
            }
            DeleteLine => {
                self.motion(SelectLine);
                self.command(CutSelection);
                self.command(DeleteCharFront);
            }
            InsertLineBreak => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    let end = Vec::from(&self.lines[cursor.line][cursor.col..]);
                    self.lines[cursor.line].drain(cursor.col..);
                    self.lines.insert(cursor.line + 1, end);
                    cursor.line += 1;
                    cursor.col = 0;
                });
            }
            InsertChar(c) => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    self.lines[cursor.line].insert(cursor.col, c);
                    cursor.col += 1;
                });
            }
            DeleteCharBack => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    let line_length = self.lines[cursor.line].len();
                    if cursor.col == 0 {
                        if cursor.line == 0 {
                            return;
                        }
                        let end = self.lines[cursor.line].clone();
                        cursor.col = self.lines[cursor.line - 1].len();
                        self.lines[cursor.line - 1].push_str(&end);
                        self.lines.remove(cursor.line);
                        cursor.line -= 1;
                    } else {
                        self.lines[cursor.line].remove(cursor.col - 1);
                        cursor.col -= 1;
                    }
                });
            }
            DeleteCharFront => {
                self.motion(ForwardOnceWrapping);
                self.command(DeleteCharBack);
            }
        }

        if self.mode == Insert || self.mode == Normal {
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
        }
    }

    fn merge_cursors(&mut self) {
        let mut merged = vec![];
        let mut current_cursor = *self.cursors.first().unwrap();

        // Since we are always moving all cursors at once, cursors can only merge in the "same direction",
        for cursor in &self.cursors[1..] {
            if cursors_overlapping(&current_cursor, cursor) {
                if cursor.moving_forward() {
                    current_cursor.line = cursor.line;
                    current_cursor.col = cursor.col;
                } else {
                    current_cursor.anchor_line = cursor.anchor_line;
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

fn is_prefix_of_command(str: &str, mode: BufferMode) -> bool {
    match mode {
        BufferMode::Normal => {
            NORMAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
                || (str.starts_with('f') && str.len() <= 2)
                || (str.starts_with('F') && str.len() <= 2)
                || (str.starts_with('r') && str.len() <= 2)
                || (str.starts_with('t') && str.len() <= 2)
                || (str.starts_with('T') && str.len() <= 2)
        }
        BufferMode::Visual | BufferMode::VisualLine => {
            VISUAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
                || (str.starts_with('f') && str.len() <= 2)
                || (str.starts_with('F') && str.len() <= 2)
                || (str.starts_with('t') && str.len() <= 2)
                || (str.starts_with('T') && str.len() <= 2)
        }
        _ => false,
    }
}

const NORMAL_MODE_COMMANDS: [&str; 16] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "dd", "J", "K", "v", "V",
];
const VISUAL_MODE_COMMANDS: [&str; 12] =
    ["j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "d"];

const SPACES_PER_TAB: usize = 4;

#[derive(Copy, Clone, PartialEq)]
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
    ForwardOnceWrapping,
    ToStartOfLine,
    ToEndOfLine,
    ToStartOfFile,
    ToEndOfFile,
    ToFirstNonBlankChar,
    ForwardToCharInclusive(u8),
    BackwardToCharInclusive(u8),
    ForwardToCharExclusive(u8),
    BackwardToCharExclusive(u8),
    SelectLine,
}

pub enum BufferCommand {
    InsertCursorAbove,
    InsertCursorBelow,
    ReplaceChar(u8),
    CutChar,
    CutSelection,
    CutLineSelection,
    DeleteLine,
    InsertLineBreak,
    InsertChar(u8),
    DeleteCharBack,
    DeleteCharFront,
}

pub enum DeviceInput {
    MouseWheel(isize),
}
