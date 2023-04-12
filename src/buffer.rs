use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
    str::pattern::Pattern,
};

use winit::event::{ModifiersState, VirtualKeyCode};
use BufferCommand::*;
use BufferMode::*;
use CursorMotion::*;
use VirtualKeyCode::{Back, Delete, Escape, Return, Tab, R};

use crate::{
    cursor::{cursors_foreach_rebalance, cursors_overlapping, Cursor},
    language_server::LanguageServer,
    language_server_types::{DidOpenTextDocumentParams, TextDocumentItem},
    language_support::{language_from_path, Language},
    piece_table::{Piece, PieceTable},
};

#[derive(Copy, Clone, PartialEq)]
pub enum BufferMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

#[derive(Clone, Debug)]
pub struct BufferState {
    pieces: Vec<Piece>,
    cursors: Vec<Cursor>,
}

pub struct Buffer {
    pub path: String,
    pub uri: String,
    pub language: &'static Language,
    pub piece_table: PieceTable,
    pub cursors: Vec<Cursor>,
    pub undo_stack: Vec<BufferState>,
    pub redo_stack: Vec<BufferState>,
    pub mode: BufferMode,
    language_server: Option<Rc<RefCell<LanguageServer>>>,
    input: String,
    version: i32,
}

impl Buffer {
    // TODO: Error handling
    pub fn new(path: &str, language_server: Option<Rc<RefCell<LanguageServer>>>) -> Self {
        let uri = "file:///".to_string() + path;
        let language = language_from_path(path).unwrap();
        let piece_table = PieceTable::from_file(path);

        Self {
            path: path.to_string(),
            uri,
            language,
            piece_table,
            cursors: vec![Cursor::default()],
            undo_stack: vec![],
            redo_stack: vec![],
            mode: BufferMode::Normal,
            language_server,
            input: String::new(),
            version: 1,
        }
    }

    pub fn send_did_open(&self, server: &mut RefMut<LanguageServer>) {
        let text = self.piece_table.iter_chars().collect();

        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: self.uri.clone(),
                language_id: self.language.identifier.to_string(),
                version: 0,
                text: unsafe { String::from_utf8_unchecked(text) },
            },
        };

        server.send_notification("textDocument/didOpen", Some(open_params));
    }

    pub fn handle_key(&mut self, key_code: VirtualKeyCode, modifiers: Option<ModifiersState>) {
        match (self.mode, key_code) {
            (Normal, Escape) => self.cursors.truncate(1),
            (Insert, Escape) => {
                self.motion(Backward(1));
                self.switch_to_normal_mode();
            }
            (_, Escape) => self.switch_to_normal_mode(),

            (Insert, Back) => self.command(DeleteCharBack),
            (_, Back) => self.motion(Backward(1)),

            (Insert, Return) => self.command(InsertChar(b'\n')),
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

            (Normal, R) if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.command(Redo);
            }

            _ => (),
        }

        self.merge_cursors();
    }

    pub fn handle_char(&mut self, c: char) {
        if self.mode == Insert {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                self.command(InsertChar(c as u8));
            }
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
            self.merge_cursors();
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

            (Normal, "x") => {
                self.push_undo_state();
                self.command(CutChar);
            }
            (Visual, "x") => {
                self.push_undo_state();
                self.command(CutSelection);
            }
            (VisualLine, "x") => {
                self.push_undo_state();
                self.command(CutLineSelection);
            }

            (Visual, "d") => {
                self.push_undo_state();
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, "d") => {
                self.push_undo_state();
                self.command(CutLineSelection);
                self.switch_to_normal_mode();
            }

            (Normal, "dd") => {
                self.push_undo_state();
                self.switch_to_visual_mode();
                self.command(DeleteLine);
                self.switch_to_normal_mode();
            }
            (Normal, "J") => self.command(InsertCursorBelow),
            (Normal, "K") => self.command(InsertCursorAbove),
            (Normal, s) if s.starts_with('r') && s.len() == 2 => {
                let c = s.chars().nth(1).unwrap() as u8;
                self.push_undo_state();
                self.command(ReplaceChar(c));
            }
            (Normal, "i") => {
                self.push_undo_state();
                self.switch_to_insert_mode();
            }
            (Normal, "I") => {
                self.push_undo_state();
                self.motion(ToFirstNonBlankChar);
                self.switch_to_insert_mode();
            }
            (Normal, "a") => {
                self.push_undo_state();
                self.switch_to_insert_mode();
                self.motion(Forward(1));
            }
            (Normal, "A") => {
                self.push_undo_state();
                self.switch_to_insert_mode();
                self.motion(ToEndOfLine);
                self.motion(Forward(1));
            }
            (Normal, "o") => {
                self.push_undo_state();
                self.switch_to_insert_mode();
                self.motion(ToEndOfLine);
                self.motion(Forward(1));
                self.command(InsertChar(b'\n'));
            }
            (Normal, "O") => {
                self.push_undo_state();
                self.switch_to_insert_mode();
                self.motion(ToStartOfLine);
                self.command(InsertChar(b'\n'));
                self.motion(Up(1));
            }
            (Normal, "u") => {
                self.command(Undo);
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

        self.merge_cursors();
    }

    fn motion(&mut self, motion: CursorMotion) {
        for cursor in &mut self.cursors {
            match motion {
                Forward(count) => cursor.move_forward(&self.piece_table, count),
                Backward(count) => cursor.move_backward(&self.piece_table, count),
                Up(count) => cursor.move_up(&self.piece_table, count),
                Down(count) => cursor.move_down(&self.piece_table, count),
                ForwardByWord => cursor.move_forward_by_word(&self.piece_table),
                BackwardByWord => cursor.move_backward_by_word(&self.piece_table),
                ForwardOnceWrapping => cursor.move_forward_once_wrapping(&self.piece_table),
                ToStartOfLine => cursor.move_to_start_of_line(&self.piece_table),
                ToEndOfLine => cursor.move_to_end_of_line(&self.piece_table),
                ToStartOfFile => cursor.move_to_start_of_file(),
                ToEndOfFile => cursor.move_to_end_of_file(&self.piece_table),
                ToFirstNonBlankChar => cursor.move_to_first_non_blank_char(&self.piece_table),
                ForwardToCharInclusive(c) => cursor.move_to_char_inc(&self.piece_table, c),
                BackwardToCharInclusive(c) => cursor.move_back_to_char_inc(&self.piece_table, c),
                ForwardToCharExclusive(c) => cursor.move_to_char_exc(&self.piece_table, c),
                BackwardToCharExclusive(c) => cursor.move_back_to_char_exc(&self.piece_table, c),
                SelectLine => cursor.select_line(&self.piece_table),
            }

            // Normal mode does not allow cursors to be on newlines
            if self.mode == Normal && cursor.at_line_end(&self.piece_table) {
                cursor.move_backward(&self.piece_table, 1);
            }

            // Cache the column position of the cursor when moving up or down
            match motion {
                Up(_) | Down(_) => cursor.stick_col(&self.piece_table),
                _ => cursor.unstick_col(&self.piece_table),
            }
        }

        if self.mode == Insert || self.mode == Normal {
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
        }
    }

    fn command(&mut self, command: BufferCommand) {
        match command {
            InsertCursorAbove => {
                if let Some(first_cursor) = self
                    .cursors
                    .iter()
                    .min_by(|c1, c2| c1.position.cmp(&c2.position))
                {
                    let mut cursor = *first_cursor;
                    cursor.cached_col = 0;
                    cursor.move_up(&self.piece_table, 1);
                    self.cursors.push(cursor);
                }
            }
            InsertCursorBelow => {
                if let Some(first_cursor) = self
                    .cursors
                    .iter()
                    .max_by(|c1, c2| c1.position.cmp(&c2.position))
                {
                    let mut cursor = *first_cursor;
                    cursor.cached_col = 0;
                    cursor.move_down(&self.piece_table, 1);
                    self.cursors.push(cursor);
                }
            }
            ReplaceChar(c) => {
                self.command(DeleteCharFront);
                self.command(InsertChar(c));
                self.motion(Backward(1));
            }
            CutChar => {
                self.command(DeleteCharFront);
            }
            CutSelection => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    if cursor.position < cursor.anchor {
                        self.piece_table.delete(cursor.position, cursor.anchor + 1);
                    } else {
                        self.piece_table.delete(cursor.anchor, cursor.position + 1);
                        cursor.position = cursor.anchor;
                    }
                });
            }
            CutLineSelection => {
                self.motion(ToEndOfLine);
                self.command(CutSelection);
                self.command(DeleteLine);
            }
            DeleteLine => {
                self.motion(SelectLine);
                self.command(CutSelection);
            }
            InsertChar(c) => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    self.piece_table.insert(cursor.position, &[c]);
                    cursor.position += 1;
                });
            }
            DeleteCharBack => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    self.piece_table
                        .delete(cursor.position.saturating_sub(1), cursor.position);
                    cursor.position = cursor.position.saturating_sub(1);
                });
            }
            DeleteCharFront => {
                self.motion(ForwardOnceWrapping);
                self.command(DeleteCharBack);
            }
            Undo => {
                if let Some(state) = self.undo_stack.pop() {
                    self.redo_stack.push(BufferState {
                        pieces: self.piece_table.pieces.clone(),
                        cursors: self.cursors.clone(),
                    });
                    self.piece_table.pieces = state.pieces;
                    self.cursors = state.cursors;
                }
            }
            Redo => {
                if let Some(state) = self.redo_stack.pop() {
                    self.undo_stack.push(BufferState {
                        pieces: self.piece_table.pieces.clone(),
                        cursors: self.cursors.clone(),
                    });
                    self.piece_table.pieces = state.pieces;
                    self.cursors = state.cursors;
                }
            }
        }

        for cursor in &mut self.cursors {
            if self.mode == Insert || self.mode == Normal {
                cursor.reset_anchor();
            }
            cursor.unstick_col(&self.piece_table)
        }
    }

    fn merge_cursors(&mut self) {
        let mut merged = vec![];
        let mut current_cursor = *self.cursors.first().unwrap();

        // Since we are always moving all cursors at once, cursors can only merge in the "same direction",
        for cursor in &self.cursors[1..] {
            if cursors_overlapping(&current_cursor, cursor) {
                if cursor.moving_forward() {
                    current_cursor.position = cursor.position;
                } else {
                    current_cursor.anchor = cursor.anchor;
                }
            } else {
                merged.push(current_cursor);
                current_cursor = *cursor;
            }
        }
        merged.push(current_cursor);

        self.cursors = merged;
    }

    fn push_undo_state(&mut self) {
        let mut cursors = self.cursors.clone();
        for cursor in &mut cursors {
            cursor.position = cursor.anchor;
        }
        self.undo_stack.push(BufferState {
            pieces: self.piece_table.pieces.clone(),
            cursors,
        });
    }

    fn switch_to_normal_mode(&mut self) {
        self.mode = Normal;
        self.input.clear();
        for cursor in &mut self.cursors {
            cursor.reset_anchor();
        }
    }

    fn switch_to_insert_mode(&mut self) {
        self.mode = Insert;
        for cursor in &mut self.cursors {
            cursor.reset_anchor();
        }
    }

    fn switch_to_visual_mode(&mut self) {
        self.mode = Visual;
        self.input.clear();
    }

    fn switch_to_visual_line_mode(&mut self) {
        self.mode = VisualLine;
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

const NORMAL_MODE_COMMANDS: [&str; 17] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "dd", "J", "K", "v", "V", "u",
];
const VISUAL_MODE_COMMANDS: [&str; 12] =
    ["j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "d"];

const SPACES_PER_TAB: usize = 4;

enum CursorMotion {
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

#[derive(PartialEq)]
enum BufferCommand {
    InsertCursorAbove,
    InsertCursorBelow,
    ReplaceChar(u8),
    CutChar,
    CutSelection,
    CutLineSelection,
    DeleteLine,
    InsertChar(u8),
    DeleteCharBack,
    DeleteCharFront,
    Undo,
    Redo,
}
