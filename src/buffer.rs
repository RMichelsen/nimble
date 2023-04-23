use std::{
    cell::{RefCell, RefMut},
    cmp::min,
    rc::Rc,
    str::pattern::Pattern,
};

use winit::event::{ModifiersState, VirtualKeyCode};
use BufferCommand::*;
use BufferMode::*;
use CursorMotion::*;
use VirtualKeyCode::{Back, Delete, Escape, Return, Space, Tab, J, K, R};

use crate::{
    cursor::{cursors_foreach_rebalance, cursors_overlapping, CompletionRequest, Cursor},
    language_server::LanguageServer,
    language_server_types::{
        CompletionParams, DidChangeTextDocumentParams, DidOpenTextDocumentParams, Position, Range,
        TextDocumentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
    },
    language_support::{language_from_path, Language},
    piece_table::{Piece, PieceTable},
    view::View,
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
    pub language_server: Option<Rc<RefCell<LanguageServer>>>,
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

    pub fn handle_key(
        &mut self,
        key_code: VirtualKeyCode,
        modifiers: Option<ModifiersState>,
        view: &View,
        num_rows: usize,
        num_cols: usize,
    ) {
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

            (Normal, Delete) => self.command(CutSelection),
            (Visual, Delete) => {
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, Delete) => {
                self.command(CutLineSelection);
                self.switch_to_normal_mode();
            }
            (Insert, Delete) => self.command(CutSelection),

            (Normal, R) if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.command(Redo);
            }

            (Insert, J) if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                for cursor in &mut self.cursors {
                    if let Some(ref mut request) = cursor.completion_request {
                        if let Some(server) = &self.language_server {
                            if let Some(completion) =
                                server.borrow().saved_completions.get(&request.id)
                            {
                                if let Some(completion_view) = view.get_completion_view(
                                    &self.piece_table,
                                    completion,
                                    request.position,
                                    num_rows,
                                    num_cols,
                                ) {
                                    request.selection_index = min(
                                        request.selection_index + 1,
                                        completion.items.len().saturating_sub(1),
                                    );

                                    if request.selection_index
                                        >= request.selection_view_offset + completion_view.height
                                    {
                                        request.selection_view_offset += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (Insert, K) if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                for cursor in &mut self.cursors {
                    if let Some(ref mut request) = cursor.completion_request {
                        request.selection_index = request.selection_index.saturating_sub(1);
                        if request.selection_index < request.selection_view_offset {
                            request.selection_view_offset -= 1;
                        }
                    }
                }
            }

            (Insert, Tab)
                if self
                    .cursors
                    .last()
                    .is_some_and(|cursor| cursor.completion_request.is_some()) =>
            {
                self.command(Complete);
            }
            (Insert, Tab) => {
                for _ in 0..SPACES_PER_TAB {
                    self.command(InsertChar(b' '));
                }
            }

            (Insert, Space) if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.command(StartCompletion);
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
                self.command(CutSelection);
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
                self.command(CutLineSelection);
                self.switch_to_normal_mode();
            }
            (Normal, "D") => {
                self.push_undo_state();
                self.switch_to_visual_mode();
                self.motion(ToEndOfLine);
                self.motion(Backward(1));
                self.command(CutSelection);
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
                ToStartOfLine => cursor.move_to_start_of_line(&self.piece_table),
                ToEndOfLine => cursor.move_to_end_of_line(&self.piece_table),
                ToStartOfFile => cursor.move_to_start_of_file(),
                ToEndOfFile => cursor.move_to_end_of_file(&self.piece_table),
                ToFirstNonBlankChar => cursor.move_to_first_non_blank_char(&self.piece_table),
                ForwardToCharInclusive(c) => cursor.move_to_char_inc(&self.piece_table, c),
                BackwardToCharInclusive(c) => cursor.move_back_to_char_inc(&self.piece_table, c),
                ForwardToCharExclusive(c) => cursor.move_to_char_exc(&self.piece_table, c),
                BackwardToCharExclusive(c) => cursor.move_back_to_char_exc(&self.piece_table, c),
                ExtendSelection => cursor.extend_selection(&self.piece_table),
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
                self.command(CutSelection);
                self.command(InsertChar(c));
                self.motion(Backward(1));
            }
            CutSelection => {
                let num_chars = self.piece_table.num_chars();
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    if cursor.position < cursor.anchor {
                        lsp_delete(
                            &mut self.language_server,
                            &self.piece_table,
                            &self.uri,
                            &mut self.version,
                            cursor.position,
                            min(cursor.anchor + 1, num_chars),
                        );
                        self.piece_table.delete(cursor.position, cursor.anchor + 1);
                    } else {
                        lsp_delete(
                            &mut self.language_server,
                            &self.piece_table,
                            &self.uri,
                            &mut self.version,
                            cursor.anchor,
                            min(cursor.position + 1, num_chars),
                        );
                        self.piece_table.delete(cursor.anchor, cursor.position + 1);
                        cursor.position = cursor.anchor;
                    }
                });
            }
            CutLineSelection => {
                self.motion(ExtendSelection);
                self.command(CutSelection);
            }
            InsertChar(c) => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    self.piece_table.insert(cursor.position, &[c]);
                    lsp_insert(
                        &mut self.language_server,
                        &self.piece_table,
                        &self.uri,
                        &mut self.version,
                        cursor.position,
                        unsafe { std::str::from_utf8_unchecked(&[c]) },
                    );
                    lsp_complete(
                        cursor,
                        Some(c),
                        &mut self.language_server,
                        &self.piece_table,
                        &self.uri,
                        cursor.position + 1,
                    );
                    cursor.position += 1;
                });
            }
            DeleteCharBack => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    let new_position = cursor.position.saturating_sub(1);
                    lsp_delete(
                        &mut self.language_server,
                        &self.piece_table,
                        &self.uri,
                        &mut self.version,
                        new_position,
                        cursor.position,
                    );
                    self.piece_table.delete(new_position, cursor.position);
                    cursor.position = new_position;
                });
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
                lsp_reload(
                    &mut self.language_server,
                    &self.piece_table,
                    &self.uri,
                    &mut self.version,
                );
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
                lsp_reload(
                    &mut self.language_server,
                    &self.piece_table,
                    &self.uri,
                    &mut self.version,
                );
            }
            StartCompletion => {
                for cursor in &mut self.cursors {
                    lsp_complete(
                        cursor,
                        None,
                        &mut self.language_server,
                        &self.piece_table,
                        &self.uri,
                        cursor.position,
                    );
                }
            }
            Complete => {
                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    if let Some(ref mut request) = cursor.completion_request {
                        let item =
                            self.language_server.as_ref().and_then(|server| {
                                server.borrow().saved_completions.get(&request.id).map(
                                    |completion| completion.items[request.selection_index].clone(),
                                )
                            });
                        if let Some(item) = item {
                            if let Some(text_edit) = item.text_edit {
                                let start = self
                                    .piece_table
                                    .char_index_from_line_col(
                                        text_edit.range.start.line as usize,
                                        text_edit.range.start.character as usize,
                                    )
                                    .unwrap_or(cursor.position);

                                // The end of the completion is the original text edit range
                                // plus the difference in cursor position
                                // (from when the completion was triggered until now)
                                let end = self
                                    .piece_table
                                    .char_index_from_line_col(
                                        text_edit.range.end.line as usize,
                                        text_edit.range.end.character as usize,
                                    )
                                    .unwrap_or(cursor.position)
                                    + (cursor.position - request.position);

                                lsp_delete(
                                    &mut self.language_server,
                                    &self.piece_table,
                                    &self.uri,
                                    &mut self.version,
                                    start,
                                    end,
                                );
                                self.piece_table.delete(start, end);
                                cursor.position = start;
                            }
                        }
                    }
                });

                cursors_foreach_rebalance(&mut self.cursors, |cursor| {
                    if let Some(ref mut request) = cursor.completion_request {
                        let item =
                            self.language_server.as_ref().and_then(|server| {
                                server.borrow().saved_completions.get(&request.id).map(
                                    |completion| completion.items[request.selection_index].clone(),
                                )
                            });
                        if let Some(item) = item {
                            if let Some(text_edit) = item.text_edit {
                                lsp_insert(
                                    &mut self.language_server,
                                    &self.piece_table,
                                    &self.uri,
                                    &mut self.version,
                                    cursor.position,
                                    &text_edit.new_text,
                                );
                                self.piece_table
                                    .insert(cursor.position, text_edit.new_text.as_bytes());
                                cursor.position += text_edit.new_text.len();
                            }
                        }
                    }
                    cursor.completion_request = None;
                });
            }
        }

        for cursor in &mut self.cursors {
            // Normal mode does not allow cursors to be on newlines
            if self.mode == Normal && cursor.at_line_end(&self.piece_table) {
                cursor.move_backward(&self.piece_table, 1);
            }

            // Remove completion requests if cursor is behind the request position
            if self.mode == Insert
                && cursor
                    .completion_request
                    .is_some_and(|request| request.position > cursor.position)
            {
                if let Some(server) = &self.language_server {
                    server
                        .borrow_mut()
                        .saved_completions
                        .remove(&cursor.completion_request.unwrap().id);
                }
                cursor.completion_request = None;
            }
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
            if cursor.at_line_end(&self.piece_table) {
                cursor.move_backward(&self.piece_table, 1);
            }

            if let Some(request) = cursor.completion_request {
                if let Some(server) = &self.language_server {
                    server.borrow_mut().saved_completions.remove(&request.id);
                }
            }

            cursor.completion_request = None;
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

fn lsp_reload(
    language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    piece_table: &PieceTable,
    uri: &str,
    version: &mut i32,
) {
    if let Some(server) = &language_server {
        let change_params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.to_string(),
                version: *version,
            },
            content_changes: vec![TextDocumentChangeEvent {
                range: None,
                text: unsafe { String::from_utf8_unchecked(piece_table.iter_chars().collect()) },
            }],
        };
        server
            .borrow_mut()
            .send_notification("textDocument/didChange", change_params);
        *version += 1;
    }
}

fn lsp_complete(
    cursor: &mut Cursor,
    character: Option<u8>,
    language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    piece_table: &PieceTable,
    uri: &str,
    position: usize,
) {
    if let Some(server) = &language_server {
        if character.is_none()
            || server
                .borrow()
                .trigger_characters
                .contains(&character.unwrap())
        {
            let (line, col) = (
                piece_table.line_index(position),
                piece_table.col_index(position),
            );
            let completion_params = CompletionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri.to_string(),
                },
                position: Position {
                    line: line as u32,
                    character: col as u32,
                },
            };
            if let Some(id) = server
                .borrow_mut()
                .send_request("textDocument/completion", completion_params)
            {
                cursor.completion_request = Some(CompletionRequest {
                    id,
                    position,
                    selection_index: 0,
                    selection_view_offset: 0,
                });
            }
        }
    }
}

fn lsp_insert(
    language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    piece_table: &PieceTable,
    uri: &str,
    version: &mut i32,
    pos: usize,
    text: &str,
) {
    if let Some(server) = &language_server {
        let (line, col) = (piece_table.line_index(pos), piece_table.col_index(pos));

        let change_params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.to_string(),
                version: *version,
            },
            content_changes: vec![TextDocumentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                }),
                text: text.to_string(),
            }],
        };
        server
            .borrow_mut()
            .send_notification("textDocument/didChange", change_params);
        *version += 1;
    }
}

fn lsp_delete(
    language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    piece_table: &PieceTable,
    uri: &str,
    version: &mut i32,
    pos1: usize,
    pos2: usize,
) {
    if let Some(server) = &language_server {
        let (line1, col1) = (piece_table.line_index(pos1), piece_table.col_index(pos1));
        let (line2, col2) = (piece_table.line_index(pos2), piece_table.col_index(pos2));

        let change_params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.to_string(),
                version: *version,
            },
            content_changes: vec![TextDocumentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: line1 as u32,
                        character: col1 as u32,
                    },
                    end: Position {
                        line: line2 as u32,
                        character: col2 as u32,
                    },
                }),
                text: String::new(),
            }],
        };
        server
            .borrow_mut()
            .send_notification("textDocument/didChange", change_params);
        *version += 1;
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

const NORMAL_MODE_COMMANDS: [&str; 18] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "dd", "D", "J", "K", "v", "V", "u",
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
    ToStartOfLine,
    ToEndOfLine,
    ToStartOfFile,
    ToEndOfFile,
    ToFirstNonBlankChar,
    ForwardToCharInclusive(u8),
    BackwardToCharInclusive(u8),
    ForwardToCharExclusive(u8),
    BackwardToCharExclusive(u8),
    ExtendSelection,
}

#[derive(PartialEq)]
enum BufferCommand {
    InsertCursorAbove,
    InsertCursorBelow,
    ReplaceChar(u8),
    CutSelection,
    CutLineSelection,
    InsertChar(u8),
    DeleteCharBack,
    Undo,
    Redo,
    StartCompletion,
    Complete,
}
