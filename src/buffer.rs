use std::{
    cmp::{max, min},
    fs::File,
    io::BufReader,
    str::pattern::Pattern,
};

use bstr::{io::BufReadExt, ByteVec};
use winit::event::VirtualKeyCode;

use crate::{
    cursor::{cursors_foreach_rebalance, cursors_overlapping, ColChange, Cursor, LineChange},
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
    // TODO: Error handling
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
                    VirtualKeyCode::Escape => {
                        self.motion(CursorMotion::Backward(1));
                        self.switch_to_normal_mode();
                    }
                    VirtualKeyCode::Back => {
                        self.insert_command(InsertBufferCommand::DeleteCharBack)
                    }
                    VirtualKeyCode::Delete => {
                        self.insert_command(InsertBufferCommand::DeleteCharFront)
                    }
                    VirtualKeyCode::Return => {
                        self.insert_command(InsertBufferCommand::InsertLineBreak)
                    }
                    VirtualKeyCode::Tab => {
                        for _ in 0..SPACES_PER_TAB {
                            self.insert_command(InsertBufferCommand::InsertChar(b' '));
                        }
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
                    s if s.starts_with('f') && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with('F') && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with('t') && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with('T') && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }

                    "x" => self.normal_command(NormalBufferCommand::CutSelection),
                    "dd" => self.normal_command(NormalBufferCommand::DeleteLine),
                    "J" => self.normal_command(NormalBufferCommand::InsertCursorBelow),
                    "K" => self.normal_command(NormalBufferCommand::InsertCursorAbove),
                    s if s.starts_with('r') && s.len() == 2 => {
                        self.normal_command(NormalBufferCommand::ReplaceChar(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }

                    "i" => {
                        self.switch_to_insert_mode();
                    }
                    "I" => {
                        self.motion(CursorMotion::ToFirstNonBlankChar);
                        self.switch_to_insert_mode();
                    }
                    "a" => {
                        for cursor in &mut self.cursors {
                            if !self.lines[cursor.line].is_empty() {
                                cursor.col += 1;
                            }
                        }
                        self.switch_to_insert_mode();
                    }
                    "A" => {
                        self.motion(CursorMotion::ToEndOfLine);
                        for cursor in &mut self.cursors {
                            if !self.lines[cursor.line].is_empty() {
                                cursor.col += 1;
                            }
                        }
                        self.switch_to_insert_mode();
                    }
                    "o" => {
                        self.normal_command(NormalBufferCommand::InsertLineBelow);
                        self.motion(CursorMotion::Down(1));
                        self.motion(CursorMotion::ToStartOfLine);
                        self.switch_to_insert_mode();
                    }
                    "O" => {
                        self.normal_command(NormalBufferCommand::InsertLineAbove);
                        self.motion(CursorMotion::ToStartOfLine);
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
                    s if s.starts_with('f') && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with('F') && s.len() == 2 => {
                        self.motion(CursorMotion::BackwardToCharInclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with('t') && s.len() == 2 => {
                        self.motion(CursorMotion::ForwardToCharExclusive(
                            s.chars().nth(1).unwrap() as u8,
                        ));
                    }
                    s if s.starts_with('T') && s.len() == 2 => {
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
            // Cache the column position of the cursor when moving up or down
            match motion {
                CursorMotion::Up(_) | CursorMotion::Down(_) => cursor.stick_col(),
                _ => cursor.unstick_col(),
            }

            match motion {
                CursorMotion::Forward(count) => cursor.move_forward(&self.lines, count),
                CursorMotion::Backward(count) => cursor.move_backward(&self.lines, count),
                CursorMotion::Up(count) => cursor.move_up(&self.lines, count),
                CursorMotion::Down(count) => cursor.move_down(&self.lines, count),
                CursorMotion::ForwardByWord => cursor.move_forward_by_word(&self.lines),
                CursorMotion::BackwardByWord => cursor.move_backward_by_word(&self.lines),
                CursorMotion::ToStartOfLine => cursor.move_to_start_of_line(),
                CursorMotion::ToEndOfLine => cursor.move_to_end_of_line(&self.lines),
                CursorMotion::ToStartOfFile => cursor.move_to_start_of_file(),
                CursorMotion::ToEndOfFile => cursor.move_to_end_of_file(&self.lines),
                CursorMotion::ToFirstNonBlankChar => {
                    cursor.move_to_first_non_blank_char(&self.lines)
                }
                CursorMotion::ForwardToCharInclusive(c) => {
                    cursor.move_forward_to_char_inclusive(&self.lines, c);
                }
                CursorMotion::BackwardToCharInclusive(c) => {
                    cursor.move_backward_to_char_inclusive(&self.lines, c);
                }
                CursorMotion::ForwardToCharExclusive(c) => {
                    cursor.move_forward_to_char_exclusive(&self.lines, c);
                }
                CursorMotion::BackwardToCharExclusive(c) => {
                    cursor.move_backward_to_char_exclusive(&self.lines, c);
                }
            }
        }
    }

    pub fn normal_command(&mut self, command: NormalBufferCommand) {
        match command {
            NormalBufferCommand::InsertCursorAbove => {
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
            NormalBufferCommand::InsertCursorBelow => {
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
            NormalBufferCommand::CutSelection => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.line].is_empty() {
                        self.lines[cursor.line].remove(cursor.col);
                        cursor.col =
                            min(cursor.col, self.lines[cursor.line].len().saturating_sub(1));
                    }
                }
            }
            NormalBufferCommand::ReplaceChar(c) => {
                for cursor in &mut self.cursors {
                    if !self.lines[cursor.line].is_empty() {
                        self.lines[cursor.line][cursor.col] = c;
                    }
                }
            }
            NormalBufferCommand::DeleteLine => {
                for (deleted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    // Special case: if only the last line remains, simply delete its content.
                    if cursor.line - deleted_lines == 0 && self.lines.len() == 1 {
                        self.lines[0].clear();
                    } else {
                        self.lines.remove(cursor.line - deleted_lines);
                    }

                    cursor.line = cursor.line.saturating_sub(deleted_lines);
                    cursor.move_to_first_non_blank_char(&self.lines);
                }
            }
            NormalBufferCommand::InsertLineAbove => {
                for (insterted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    cursor.line += insterted_lines;
                    self.lines.insert(cursor.line, vec![]);
                }
            }
            NormalBufferCommand::InsertLineBelow => {
                for (insterted_lines, cursor) in self.cursors.iter_mut().enumerate() {
                    cursor.line += insterted_lines;
                    self.lines.insert(cursor.line + 1, vec![]);
                }
            }
        }
    }
    pub fn visual_command(&mut self, command: VisualBufferCommand) {
        match command {
            VisualBufferCommand::CutSelection => {
                cursors_foreach_rebalance(self.cursors.as_mut_slice(), |cursor| {
                    let selection_ranges = cursor.get_selection_ranges(&self.lines);
                    if selection_ranges.len() == 1 {
                        if let Some(range) = selection_ranges.first() {
                            if !self.lines[range.line].is_empty() {
                                self.lines[range.line].drain(range.start..=range.end);

                                cursor.col = range.start;
                                cursor.anchor_col = range.start;

                                return (
                                    vec![],
                                    vec![ColChange::Removed(
                                        cursor.line,
                                        range.end - range.start + 1,
                                    )],
                                );
                            }
                        }
                    }
                    if selection_ranges.len() > 1 {
                        let mut col_changes = vec![];
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
                            col_changes.push(ColChange::Removed(cursor.line, end.len()));
                        }

                        let mut line_changes = vec![];
                        for range in selection_ranges[1..].iter().rev() {
                            self.lines.remove(range.line);
                            line_changes.push(LineChange::Removed(range.line));
                        }

                        return (line_changes, vec![]);
                    }

                    (vec![], vec![])
                });

                self.switch_to_normal_mode();
            }
        }
    }

    pub fn visual_line_command(&mut self, command: VisualLineBufferCommand) {
        match command {
            VisualLineBufferCommand::CutSelection => {
                cursors_foreach_rebalance(self.cursors.as_mut_slice(), |cursor| {
                    let first_line = min(cursor.line, cursor.anchor_line);
                    let last_line = max(cursor.line, cursor.anchor_line);
                    cursor.line = first_line;
                    cursor.anchor_line = cursor.line;
                    cursor.col = 0;
                    cursor.anchor_col = 0;

                    let mut line_changes = vec![];
                    for line in (first_line..=last_line).rev() {
                        self.lines.remove(line);
                        line_changes.push(LineChange::Removed(line));
                    }

                    (line_changes, vec![])
                });

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
                    self.lines[cursor.line].insert(cursor.col, c);
                    cursor.col += 1;
                }
            }
            InsertBufferCommand::DeleteCharBack => {
                cursors_foreach_rebalance(self.cursors.as_mut_slice(), |cursor| {
                    let line_length = self.lines[cursor.line].len();
                    if cursor.col == 0 {
                        if cursor.line == 0 {
                            return (vec![], vec![]);
                        }
                        let end = self.lines[cursor.line].clone();
                        cursor.col = self.lines[cursor.line - 1].len();
                        self.lines[cursor.line - 1].push_str(&end);
                        self.lines.remove(cursor.line);
                        let changes = (vec![LineChange::Removed(cursor.line)], vec![]);
                        cursor.line -= 1;
                        changes
                    } else {
                        self.lines[cursor.line].remove(cursor.col - 1);
                        cursor.col -= 1;
                        (vec![], vec![ColChange::Removed(cursor.line, 1)])
                    }
                });
            }
            InsertBufferCommand::DeleteCharFront => {
                cursors_foreach_rebalance(self.cursors.as_mut_slice(), |cursor| {
                    let line_length = self.lines[cursor.line].len();
                    if cursor.col == line_length || line_length == 0 {
                        if cursor.line == self.lines.len().saturating_sub(1) {
                            return (vec![], vec![]);
                        }
                        let end = self.lines[cursor.line + 1].clone();
                        let changes = (
                            vec![LineChange::Removed(cursor.line + 1)],
                            vec![ColChange::Inserted(
                                cursor.line,
                                self.lines[cursor.line].len(),
                            )],
                        );
                        self.lines[cursor.line].push_str(&end);
                        self.lines.remove(cursor.line + 1);

                        changes
                    } else {
                        self.lines[cursor.line].remove(cursor.col);
                        (vec![], vec![ColChange::Removed(cursor.line, 1)])
                    }
                });
            }
            InsertBufferCommand::InsertLineBreak => {
                cursors_foreach_rebalance(self.cursors.as_mut_slice(), |cursor| {
                    let end = Vec::from(&self.lines[cursor.line][cursor.col..]);
                    self.lines[cursor.line].drain(cursor.col..);
                    self.lines.insert(cursor.line + 1, end);
                    let changes = (
                        vec![LineChange::Inserted(cursor.line)],
                        vec![ColChange::Removed(
                            cursor.line + 1,
                            self.lines[cursor.line].len(),
                        )],
                    );

                    cursor.line += 1;
                    cursor.col = 0;

                    changes
                });
            }
        }
    }

    fn merge_cursors(&mut self) {
        let mut merged = vec![];
        let mut current_cursor = *self.cursors.first().unwrap();

        // Since we are always moving all cursors at once, cursors can only merge in the "same direction"
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

fn is_prefix_of_normal_command(str: &str) -> bool {
    NORMAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
        || (str.starts_with('f') && str.len() <= 2)
        || (str.starts_with('F') && str.len() <= 2)
        || (str.starts_with('r') && str.len() <= 2)
        || (str.starts_with('t') && str.len() <= 2)
        || (str.starts_with('T') && str.len() <= 2)
}
fn is_prefix_of_visual_command(str: &str) -> bool {
    VISUAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
        || (str.starts_with('f') && str.len() <= 2)
        || (str.starts_with('F') && str.len() <= 2)
        || (str.starts_with('t') && str.len() <= 2)
        || (str.starts_with('T') && str.len() <= 2)
}

const NORMAL_MODE_COMMANDS: [&str; 16] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "dd", "J", "K", "v", "V",
];
const VISUAL_MODE_COMMANDS: [&str; 12] =
    ["j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "d"];

const SPACES_PER_TAB: usize = 4;

#[derive(PartialEq)]
pub enum BufferMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

#[derive(PartialEq)]
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
