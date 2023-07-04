use std::{
    cell::{RefCell, RefMut},
    cmp::{max, min},
    collections::VecDeque,
    rc::Rc,
    str::pattern::Pattern,
};

use bstr::ByteSlice;
use imgui_winit_support::winit::window::Window;
use url::Url;
use BufferCommand::*;
use BufferMode::*;
use CursorMotion::*;

use crate::{
    cursor::{
        cursors_delete_rebalance, cursors_insert_rebalance, cursors_overlapping,
        get_filtered_completions, CompletionRequest, Cursor, SignatureHelpRequest,
    },
    editor::EditorCommand,
    language_server::LanguageServer,
    language_server_types::{
        CompletionParams, DefinitionParams, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
        HoverParams, ImplementationParams, Position, Range, SignatureHelpContext,
        SignatureHelpParams, TextDocumentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
    },
    language_support::{language_from_path, Language},
    piece_table::{Piece, PieceTable},
    platform_resources::PlatformResources,
    syntect::{IndexedLine, Syntect, SYNTECT_CACHE_FREQUENCY},
    text_utils::{self},
    theme::Theme,
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
    pub language: Option<&'static Language>,
    pub piece_table: PieceTable,
    pub cursors: Vec<Cursor>,
    pub undo_stack: Vec<BufferState>,
    pub redo_stack: Vec<BufferState>,
    pub mode: BufferMode,
    pub language_server: Option<Rc<RefCell<LanguageServer>>>,
    pub syntect: Option<Syntect>,
    pub input: String,
    last_executed_command: Option<String>,
    insertion_command_stack: Vec<BufferCommand>,
    insertion_stack_dirty: bool,
    highlight_queue: VecDeque<usize>,
    search_string: String,
    search_anchor: usize,
    version: i32,
    platform_resources: PlatformResources,
}

impl Buffer {
    pub fn new(
        window: &Window,
        uri: &Url,
        theme: &Theme,
        language_server: Option<Rc<RefCell<LanguageServer>>>,
    ) -> Self {
        let path = uri.to_file_path().unwrap().to_str().unwrap().to_string();
        let language = language_from_path(&path);
        let piece_table = PieceTable::from_file(&path);

        let mut highlight_queue = VecDeque::new();
        let mut i = 0;
        while i < piece_table.num_lines() {
            highlight_queue.push_back(i);
            i += SYNTECT_CACHE_FREQUENCY;
        }

        Self {
            path: path.clone(),
            uri: uri.to_string(),
            language,
            piece_table,
            cursors: vec![Cursor::default()],
            undo_stack: vec![],
            redo_stack: vec![],
            mode: BufferMode::Normal,
            language_server,
            syntect: Syntect::new(&path, theme),
            input: String::default(),
            last_executed_command: None,
            insertion_command_stack: vec![],
            insertion_stack_dirty: false,
            highlight_queue,
            search_string: String::new(),
            search_anchor: 0,
            version: 1,
            platform_resources: PlatformResources::new(window),
        }
    }

    pub fn syntect_reload(&mut self, theme: &Theme) {
        self.syntect = Syntect::new(&self.path, theme);
        let mut i = 0;
        while i < self.piece_table.num_lines() {
            self.highlight_queue.push_back(i);
            i += SYNTECT_CACHE_FREQUENCY;
        }
    }

    pub fn send_did_open(&self, server: &mut RefMut<LanguageServer>) {
        let text = self.piece_table.iter_chars().collect();
        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: self.uri.clone(),
                language_id: self.language.unwrap().identifier.to_string(),
                version: 0,
                text: unsafe { String::from_utf8_unchecked(text) },
            },
        };

        server.send_notification("textDocument/didOpen", Some(open_params));
    }

    pub fn set_cursor(&mut self, line: usize, col: usize) {
        if let Some(mouse_line) = self.piece_table.line_at_index(line) {
            if let Some(position) = self
                .piece_table
                .char_index_from_line_col(line, min(col, mouse_line.length.saturating_sub(1)))
            {
                self.cursors.truncate(1);
                self.switch_to_normal_mode();
                self.cursors[0].position = position;
                self.cursors[0].anchor = position;
            }
        } else {
            self.cursors.truncate(1);
            self.switch_to_normal_mode();
            let last_position = self.piece_table.num_chars().saturating_sub(2);
            self.cursors[0].position = last_position;
            self.cursors[0].anchor = last_position;
        }
    }

    pub fn set_drag(&mut self, line: usize, col: usize) {
        if let Some(mouse_line) = self.piece_table.line_at_index(line) {
            if let Some(position) = self
                .piece_table
                .char_index_from_line_col(line, min(col, mouse_line.length))
            {
                // Only start visual selection if the cursor moved cell
                // Disallowing selecting '\n' on the same line by dragging
                if self.cursors[0].position == position
                    || (self.piece_table.line_index(self.cursors[0].position) == mouse_line.index
                        && self.cursors[0].position == mouse_line.end.saturating_sub(1)
                        && col >= self.piece_table.col_index(self.cursors[0].position))
                {
                    return;
                }

                if self.cursors[0].position != position {
                    self.switch_to_visual_mode();
                    self.cursors[0].position = position;
                }
            }
        }
    }

    pub fn handle_mouse_double_click(&mut self, line: usize, col: usize) -> bool {
        if let Some(cursor_line) = self.piece_table.line_at_index(line) {
            if let Some(position) = self
                .piece_table
                .char_index_from_line_col(line, min(col, cursor_line.length.saturating_sub(1)))
            {
                if self.cursors[0].position == position {
                    self.switch_to_visual_mode();
                    self.motion(ExtendSelectionInside(b'w'));
                    return true;
                }
            }
        }
        self.set_cursor(line, col);
        false
    }

    pub fn handle_mouse_hover(&mut self, line: usize, col: usize) {
        if let Some(cursor_line) = self.piece_table.line_at_index(line) {
            if col >= cursor_line.length {
                return;
            }
            self.lsp_hover(line, col);
        }
    }

    pub fn insert_cursor(&mut self, line: usize, col: usize) {
        if let Some(cursor_line) = self.piece_table.line_at_index(line) {
            if let Some(position) = self
                .piece_table
                .char_index_from_line_col(line, min(col, cursor_line.length))
            {
                self.cursors.push(Cursor::new(position));
            }
        }
    }

    pub fn handle_key(&mut self, key: imgui::Key, ctrl_down: bool) -> Option<EditorCommand> {
        match (self.mode, key) {
            (_, imgui::Key::DownArrow) => self.motion(Down(1)),
            (_, imgui::Key::UpArrow) => self.motion(Up(1)),
            (_, imgui::Key::RightArrow) if ctrl_down => self.motion(ForwardByWord),
            (_, imgui::Key::LeftArrow) if ctrl_down => self.motion(BackwardByWord),
            (_, imgui::Key::RightArrow) => self.motion(Forward(1)),
            (_, imgui::Key::LeftArrow) => self.motion(Backward(1)),

            (Normal, imgui::Key::Escape) if self.input.as_bytes().first() == Some(&b'/') => {
                self.input.clear();
                self.cursors[0].position = self.search_anchor;
                self.cursors[0].anchor = self.search_anchor;
                return Some(EditorCommand::CenterIfNotVisible);
            }
            (Normal, imgui::Key::Escape) => {
                self.cursors.truncate(1);
                self.input.clear();
            }
            (Insert, imgui::Key::Escape) => {
                self.motion(Backward(1));
                self.switch_to_normal_mode();
            }
            (_, imgui::Key::Escape) => self.switch_to_normal_mode(),

            (Insert, imgui::Key::Backspace) if ctrl_down => {
                self.command(DeleteWordBack);
            }
            (Insert, imgui::Key::Backspace) => self.command(DeleteCharBack),
            (_, imgui::Key::Backspace) => {
                if self
                    .input
                    .as_bytes()
                    .first()
                    .is_some_and(|c| *c == b':' || *c == b'/')
                {
                    self.input.pop();
                } else {
                    self.motion(Backward(1));
                }
            }

            (Insert, imgui::Key::Enter) => self.command(InsertNewLine),
            (_, imgui::Key::Enter) => {
                if self
                    .input
                    .as_bytes()
                    .first()
                    .is_some_and(|c| *c == b':' || *c == b'/')
                {
                    let editor_command = self.handle_input_command();
                    self.input.clear();
                    return editor_command;
                } else {
                    self.motion(Down(1));
                }
            }

            (Normal, imgui::Key::Delete) => {
                self.command(CopySelection);
                self.command(CutSelection);
            }
            (Visual, imgui::Key::Delete) => {
                self.command(CopySelection);
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, imgui::Key::Delete) => {
                self.motion(ExtendSelection);
                self.command(CopySelection);
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (Insert, imgui::Key::Delete) if ctrl_down => {
                self.command(DeleteWordFront);
            }
            (Insert, imgui::Key::Delete) => self.command(CutSingleSelection),

            (Normal, imgui::Key::R) if ctrl_down => {
                self.command(Redo);
            }

            (Insert, imgui::Key::J) if ctrl_down => {
                for cursor in &mut self.cursors {
                    if let Some(ref mut request) = cursor.completion_request {
                        if let Some(server) = &self.language_server {
                            if let Some(completion_list) =
                                server.borrow().saved_completions.get(&request.id)
                            {
                                let filtered_completions = get_filtered_completions(
                                    &self.piece_table,
                                    completion_list,
                                    request,
                                    cursor.position,
                                );

                                // if let Some(completion_view) = view.get_completion_view(
                                //     &self.piece_table,
                                //     &filtered_completions,
                                //     request.position,
                                //     layout,
                                // ) {
                                //     request.selection_index = min(
                                //         request.selection_index + 1,
                                //         filtered_completions.len().saturating_sub(1),
                                //     );

                                //     if request.selection_index
                                //         >= request.selection_view_offset + completion_view.height
                                //     {
                                //         request.selection_view_offset += 1;
                                //     }
                                // }
                            }
                        }
                    }
                }
            }
            (Insert, imgui::Key::K) if ctrl_down => {
                for cursor in &mut self.cursors {
                    if let Some(ref mut request) = cursor.completion_request {
                        request.selection_index = request.selection_index.saturating_sub(1);
                        if request.selection_index < request.selection_view_offset {
                            request.selection_view_offset -= 1;
                        }
                    }
                }
            }

            (Normal | Visual | VisualLine, imgui::Key::Slash) if ctrl_down => {
                self.push_undo_state();
                self.command(ToggleComment);
            }

            (Insert, imgui::Key::Tab)
                if self
                    .cursors
                    .last()
                    .is_some_and(|cursor| cursor.completion_request.is_some()) =>
            {
                self.push_undo_state();
                self.command(Complete);
            }
            (Insert, imgui::Key::Tab) => {
                for _ in 0..self.piece_table.indent_width {
                    self.command(InsertChar(b' '));
                }
            }
            (_, imgui::Key::Tab) if ctrl_down => return Some(EditorCommand::PreviousTab),
            (_, imgui::Key::Tab) => return Some(EditorCommand::NextTab),

            (Insert, imgui::Key::Space) if ctrl_down => {
                self.command(StartCompletion);
            }

            _ => return None,
        }

        self.merge_cursors();
        None
    }

    pub fn handle_char(&mut self, c: char) -> Option<EditorCommand> {
        if self.mode == Insert {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                self.command(InsertChar(c as u8));
            }
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
            self.merge_cursors();
            return None;
        }

        if self.input.as_bytes().first() == Some(&b':') {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                self.input.push(c);
            }
            return None;
        }

        if self.input.as_bytes().first() == Some(&b'/') {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                self.input.push(c);
            }
            let partial_search = self.input[1..].to_string();
            self.motion(SeekToSelf(partial_search.as_bytes()));
            return Some(EditorCommand::CenterIfNotVisible);
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
            (_, "zz") => return Some(EditorCommand::CenterView),
            (_, "/") => {
                self.cursors.truncate(1);
                self.search_string.clear();
                self.search_anchor = self.cursors.first().unwrap().position;
                return None;
            }
            (_, "n") => {
                self.motion(SeekUntil(self.search_string.clone().as_bytes()));
                return Some(EditorCommand::CenterIfNotVisible);
            }
            (_, "N") => {
                self.motion(SeekBackUntil(self.search_string.clone().as_bytes()));
                return Some(EditorCommand::CenterIfNotVisible);
            }
            (_, "G") => self.motion(ToEndOfFile),
            (_, s) if s.starts_with('f') && s.len() == 2 => {
                self.motion(ForwardToChar(s.chars().nth(1).unwrap() as u8));
            }
            (_, s) if s.starts_with('F') && s.len() == 2 => {
                self.motion(BackwardToChar(s.chars().nth(1).unwrap() as u8));
            }
            (_, s) if s.starts_with('t') && s.len() == 2 => {
                self.motion(ForwardUntilChar(s.chars().nth(1).unwrap() as u8));
            }
            (_, s) if s.starts_with('T') && s.len() == 2 => {
                self.motion(BackwardUntilChar(s.chars().nth(1).unwrap() as u8));
            }

            (Visual, "y") => {
                self.command(CopySelection);
                for cursor in &mut self.cursors {
                    cursor.position = min(cursor.anchor, cursor.position);
                }
                self.switch_to_normal_mode();
            }
            (VisualLine, "y") => {
                self.motion(ExtendSelection);
                self.command(CopySelection);
                for cursor in &mut self.cursors {
                    cursor.position = min(cursor.anchor, cursor.position);
                }
                self.switch_to_normal_mode();
            }

            (Visual, "p") => {
                self.push_undo_state();
                self.command(CutSelection);
                self.motion(BackwardOnceWrapping);
                self.command(PasteSelection);
                self.switch_to_normal_mode();
            }
            (Visual, "P") => {
                self.push_undo_state();
                self.command(CutSelection);
                self.motion(BackwardOnceWrapping);
                self.command(PasteCursorSelection);
                self.switch_to_normal_mode();
            }

            (VisualLine, "p") => {
                self.push_undo_state();
                self.motion(ExtendSelection);
                self.command(CutSelection);
                self.motion(BackwardOnceWrapping);
                self.command(PasteSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, "P") => {
                self.push_undo_state();
                self.motion(ExtendSelection);
                self.command(CutSelection);
                self.motion(BackwardOnceWrapping);
                self.command(PasteCursorSelection);
                self.switch_to_normal_mode();
            }

            (Normal, "yy") => {
                self.switch_to_visual_mode();
                self.command(CopyLine);
                self.switch_to_normal_mode();
            }
            (Normal, "p") => {
                self.push_undo_state();
                self.command(PasteSelection);
                self.last_executed_command = Some(self.input.clone());
            }
            (Normal, "P") => {
                self.push_undo_state();
                self.command(PasteCursorSelection);
                self.last_executed_command = Some(self.input.clone());
            }

            (Normal | Visual | VisualLine, ">") => {
                self.push_undo_state();
                self.command(IndentLine);
            }

            (Normal | Visual | VisualLine, "<") => {
                self.push_undo_state();
                self.command(UnindentLine);
            }

            (Normal, s) if s.starts_with("ci") && s.len() == 3 => {
                self.last_executed_command = Some(self.input.clone());
                let c = s.chars().nth(2).unwrap() as u8;
                self.command(CutMotion(c, CutMotion::Inside, true));
            }
            (Normal, s) if s.starts_with("di") && s.len() == 3 => {
                self.last_executed_command = Some(self.input.clone());
                let c = s.chars().nth(2).unwrap() as u8;
                self.command(CutMotion(c, CutMotion::Inside, false));
            }

            (Normal, s) if s.starts_with("ct") && s.len() == 3 => {
                self.last_executed_command = Some(self.input.clone());
                let c = s.chars().nth(2).unwrap() as u8;
                self.command(CutMotion(c, CutMotion::ForwardUntil, true));
            }
            (Normal, s) if s.starts_with("dt") && s.len() == 3 => {
                self.last_executed_command = Some(self.input.clone());
                let c = s.chars().nth(2).unwrap() as u8;
                self.command(CutMotion(c, CutMotion::ForwardUntil, false));
            }
            (Normal, s) if s.starts_with("cT") && s.len() == 3 => {
                self.last_executed_command = Some(self.input.clone());
                let c = s.chars().nth(2).unwrap() as u8;
                self.command(CutMotion(c, CutMotion::BackwardUntil, true));
            }
            (Normal, s) if s.starts_with("dT") && s.len() == 3 => {
                self.last_executed_command = Some(self.input.clone());
                let c = s.chars().nth(2).unwrap() as u8;
                self.command(CutMotion(c, CutMotion::BackwardTo, false));
            }

            (Visual, s) if s.starts_with('i') && s.len() == 2 => {
                self.motion(ExtendSelectionInside(s.chars().nth(1).unwrap() as u8))
            }

            (Normal, "x") => {
                self.last_executed_command = Some(self.input.clone());
                self.push_undo_state();
                self.command(CopySelection);
                self.command(CutSelection);
            }
            (Visual, "x") => {
                self.push_undo_state();
                self.command(CopySelection);
                self.command(CutSelection);
            }
            (VisualLine, "x") => {
                self.push_undo_state();
                self.motion(ExtendSelection);
                self.command(CopySelection);
                self.command(CutSelection);
            }

            (Visual, "d") => {
                self.push_undo_state();
                self.command(CopySelection);
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (VisualLine, "d") => {
                self.push_undo_state();
                self.motion(ExtendSelection);
                self.command(CopySelection);
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }

            (Normal, "dd") => {
                self.last_executed_command = Some(self.input.clone());
                self.push_undo_state();
                self.switch_to_visual_mode();
                self.motion(ExtendSelection);
                self.command(CopySelection);
                self.command(CutSelection);
                self.switch_to_normal_mode();
            }
            (Normal, "D") => {
                self.last_executed_command = Some(self.input.clone());
                self.push_undo_state();
                self.switch_to_visual_mode();
                self.motion(ToEndOfLine);
                self.motion(Backward(1));
                self.command(CopySelection);
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
                self.command(InsertNewLine);
            }
            (Normal, "O") => {
                self.push_undo_state();
                self.switch_to_insert_mode();
                self.motion(ToStartOfLine);
                self.command(InsertNewLine);
                self.motion(Up(1));
            }
            (Normal, "u") => {
                self.command(Undo);
            }
            (Normal, ".") => {
                if let Some(command) = &self.last_executed_command {
                    if let Some(last_char) = command.as_bytes().last() {
                        self.input = command[..command.len().saturating_sub(1)].to_string();
                        let change_command = self.input.starts_with('c');
                        self.handle_char(*last_char as char);

                        if change_command {
                            self.switch_to_insert_mode();
                            let insertion_commands: Vec<BufferCommand> =
                                self.insertion_command_stack.to_vec();
                            let tmp = self.insertion_command_stack.clone();
                            for insertion_command in &insertion_commands {
                                self.command(*insertion_command);
                            }
                            self.insertion_command_stack = tmp;
                            self.motion(Backward(1));
                            self.switch_to_normal_mode();
                        }
                    }
                }
            }
            (Normal, "gd") => {
                self.command(GotoDefinition);
            }
            (Normal, "gi") => {
                self.command(GotoImplementation);
            }
            (Visual, "v") => self.switch_to_normal_mode(),
            (_, "v") => self.switch_to_visual_mode(),
            (VisualLine, "V") => self.switch_to_normal_mode(),
            (_, "V") => self.switch_to_visual_line_mode(),

            _ => return None,
        }

        if self.mode == Normal {
            for cursor in &mut self.cursors {
                cursor.reset_anchor();
            }
        }
        self.input.clear();
        self.merge_cursors();
        None
    }

    pub fn update_highlights(&mut self) -> bool {
        if let Some(syntect) = &mut self.syntect {
            if let Some(line) = self.highlight_queue.pop_front() {
                syntect.queue.lock().unwrap().push_back(IndexedLine {
                    index: line,
                    text: self
                        .piece_table
                        .text_between_lines(line, line + SYNTECT_CACHE_FREQUENCY.saturating_sub(1)),
                });
            }

            {
                use std::borrow::BorrowMut;
                let mut cache_updated = syntect.cache_updated.borrow_mut().lock().unwrap();
                if *cache_updated {
                    *cache_updated = false;
                    return true;
                }
            }
        }
        false
    }

    pub fn update_completions(&mut self, server: &mut RefMut<LanguageServer>) {
        for cursor in &mut self.cursors {
            if let Some(request) = cursor.completion_request.as_mut() {
                if request
                    .next_id
                    .is_some_and(|id| server.saved_completions.contains_key(&id))
                {
                    server.saved_completions.remove(&request.id);

                    request.id = request.next_id.unwrap();
                    request.position = request.next_position.unwrap();
                    request.next_id = None;
                    request.next_position = None;
                }
            }
        }
    }

    pub fn update_signature_helps(&mut self, server: &mut RefMut<LanguageServer>) {
        for cursor in &mut self.cursors {
            if let Some(request) = cursor.signature_help_request.as_mut() {
                if request
                    .next_id
                    .is_some_and(|id| server.saved_signature_helps.contains_key(&id))
                {
                    // If the old signature help was empty, update the position of the signature help
                    if server
                        .saved_signature_helps
                        .get(&request.id)
                        .is_some_and(|old_signature_help| old_signature_help.signatures.is_empty())
                    {
                        request.position = request.next_position.unwrap();
                    }

                    server.saved_signature_helps.remove(&request.id);

                    request.id = request.next_id.unwrap();
                    request.next_id = None;
                    request.next_position = None;
                }
            }
        }
    }

    pub fn ready_to_quit(&mut self) -> bool {
        if !self.piece_table.dirty {
            return true;
        }

        if let Some(user_wants_save) = self.platform_resources.confirm_quit(&self.path) {
            if user_wants_save {
                self.piece_table.save_to(&self.path);
            }
            return true;
        }

        false
    }

    pub fn update_syntect(&mut self, line: usize) {
        if let Some(syntect) = &mut self.syntect {
            syntect.queue.lock().unwrap().clear();
            self.highlight_queue.clear();

            let start = if let Some(last_cursor) = self.cursors.last() {
                self.piece_table.line_index(last_cursor.position) / SYNTECT_CACHE_FREQUENCY
            } else {
                0
            };

            if start > 0 {
                self.highlight_queue
                    .push_back((start - 1) * SYNTECT_CACHE_FREQUENCY);
                self.highlight_queue
                    .push_back(start * SYNTECT_CACHE_FREQUENCY);
                if start + 1 < self.piece_table.num_lines() {
                    self.highlight_queue
                        .push_back((start + 1) * SYNTECT_CACHE_FREQUENCY);
                }
            }

            let mut i = (line / SYNTECT_CACHE_FREQUENCY) * SYNTECT_CACHE_FREQUENCY;
            while i < self.piece_table.num_lines() {
                self.highlight_queue.push_back(i);
                i += SYNTECT_CACHE_FREQUENCY;
            }
        }
    }

    fn handle_input_command(&mut self) -> Option<EditorCommand> {
        let input = self.input.clone();
        match input.as_str() {
            input if input.as_bytes().first() == Some(&b'/') => {
                self.motion(SeekToSelf(input[1..].as_bytes()));
                self.search_string = input[1..].to_string();
                return Some(EditorCommand::CenterIfNotVisible);
            }
            input if let Ok(num) = input[1..].parse::<usize>() => {
                self.motion(GotoLine(num));
                self.motion(ToFirstNonBlankChar);
                return Some(EditorCommand::CenterView);
            }
            ":w" => {
                self.piece_table.save_to(&self.path);
            }
            ":wq" => {
                self.piece_table.save_to(&self.path);
                return Some(EditorCommand::Quit);
            }
            ":q" | ":bd" => {
                return Some(EditorCommand::Quit);
            }
            ":q!" | ":bd!" => {
                return Some(EditorCommand::QuitNoCheck);
            }
            ":qa" => {
                return Some(EditorCommand::QuitAll);
            }
            ":qa!" => {
                return Some(EditorCommand::QuitAllNoCheck);
            }
            ":split" => {
                return Some(EditorCommand::ToggleSplitView);
            }
            _ => ()
        }
        None
    }

    fn motion(&mut self, motion: CursorMotion) {
        for cursor in &mut self.cursors {
            match motion {
                Forward(count) => cursor.move_forward(&self.piece_table, count),
                Backward(count) => cursor.move_backward(&self.piece_table, count),
                BackwardOnceWrapping => cursor.move_backward_once_wrapping(&self.piece_table),
                Up(count) => cursor.move_up(&self.piece_table, count),
                Down(count) => cursor.move_down(&self.piece_table, count),
                ForwardByWord => cursor.move_forward_by_word(&self.piece_table),
                BackwardByWord => cursor.move_backward_by_word(&self.piece_table),
                ToStartOfLine => cursor.move_to_start_of_line(&self.piece_table),
                ToEndOfLine => cursor.move_to_end_of_line(&self.piece_table),
                ToStartOfFile => cursor.move_to_start_of_file(),
                ToEndOfFile => cursor.move_to_end_of_file(&self.piece_table),
                ToFirstNonBlankChar => cursor.move_to_first_non_blank_char(&self.piece_table),
                ForwardToChar(c) => cursor.move_to_char(&self.piece_table, c),
                BackwardToChar(c) => cursor.move_back_to_char(&self.piece_table, c),
                ForwardUntilChar(c) => cursor.move_until_char(&self.piece_table, c),
                BackwardUntilChar(c) => cursor.move_back_until_char(&self.piece_table, c),
                ExtendSelection => cursor.extend_selection(&self.piece_table),
                ExtendSelectionInside(c) => cursor.extend_selection_inside(&self.piece_table, c),
                GotoLine(n) => cursor.goto_line(&self.piece_table, n),
                SeekUntil(text) => cursor.seek(&self.piece_table, text.as_bytes(), false),
                SeekBackUntil(text) => cursor.seek_back(&self.piece_table, text.as_bytes(), false),
                SeekToSelf(text) => cursor.seek(&self.piece_table, text.as_bytes(), true),
                SeekBackToSelf(text) => cursor.seek_back(&self.piece_table, text.as_bytes(), true),
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
                let mut content_changes = vec![];

                let num_chars = self.piece_table.num_chars();
                for i in 0..self.cursors.len() {
                    content_changes.push(
                        self.delete_chars(self.cursors[i].position, self.cursors[i].position + 1),
                    );
                    content_changes.push(self.insert_chars(self.cursors[i].position, &[c]));
                }

                self.lsp_change(content_changes);
                self.syntect_change();
            }
            CutSelection => {
                let mut content_changes = vec![];

                let num_chars = self.piece_table.num_chars();
                for i in 0..self.cursors.len() {
                    if self.cursors[i].position < self.cursors[i].anchor {
                        let start = self.cursors[i].position;
                        let end = min(self.cursors[i].anchor + 1, num_chars);
                        content_changes.push(self.delete_chars(start, end));
                    } else {
                        let start = self.cursors[i].anchor;
                        let end = min(self.cursors[i].position + 1, num_chars);
                        content_changes.push(self.delete_chars(start, end));
                        self.cursors[i].position =
                            min(start, self.piece_table.num_chars().saturating_sub(1));
                    }
                }

                self.lsp_change(content_changes);
                self.syntect_change();
            }
            CutMotion(c, motion, change_command) => {
                self.push_undo_state();
                self.switch_to_visual_mode();

                let mut content_changes = vec![];
                let mut selection: Vec<u8> = vec![];

                let num_chars = self.piece_table.num_chars();
                let num_cursors = self.cursors.len();
                for i in 0..num_cursors {
                    let old_anchor = self.cursors[i].anchor;
                    let old_position = self.cursors[i].position;

                    match motion {
                        CutMotion::Inside => {
                            self.cursors[i].extend_selection_inside(&self.piece_table, c)
                        }
                        CutMotion::ForwardUntil => {
                            self.cursors[i].move_until_char(&self.piece_table, c)
                        }
                        CutMotion::ForwardTo => self.cursors[i].move_to_char(&self.piece_table, c),
                        CutMotion::BackwardUntil => {
                            self.cursors[i].move_back_until_char(&self.piece_table, c)
                        }
                        CutMotion::BackwardTo => {
                            self.cursors[i].move_back_to_char(&self.piece_table, c)
                        }
                    }

                    if self.cursors[i].position != old_position
                        || self.cursors[i].anchor != old_anchor
                    {
                        self.cursors[i].save_selection_to_clipboard(&self.piece_table);
                        selection.extend(self.cursors[i].get_selection(&self.piece_table));

                        // Insert new lines between the concatenated clipboard content in multi-cursor mode
                        if num_cursors > 1 {
                            selection.push(b'\n');
                        }

                        if self.cursors[i].position < self.cursors[i].anchor {
                            let start = self.cursors[i].position;
                            let end = min(self.cursors[i].anchor + 1, num_chars);
                            content_changes.push(self.delete_chars(start, end));
                        } else {
                            let start = self.cursors[i].anchor;
                            let end = min(self.cursors[i].position + 1, num_chars);
                            content_changes.push(self.delete_chars(start, end));
                            self.cursors[i].position =
                                min(start, self.piece_table.num_chars().saturating_sub(1));
                        }
                    }
                }

                if content_changes.is_empty() {
                    self.undo_stack.pop();
                }

                if !content_changes.is_empty() && change_command {
                    self.switch_to_insert_mode();
                } else {
                    self.switch_to_normal_mode();
                }

                if !selection.is_empty() {
                    self.platform_resources.set_clipboard(&selection);
                }

                self.lsp_change(content_changes);
                self.syntect_change();
            }
            CutSingleSelection => {
                let mut content_changes = vec![];

                let num_chars = self.piece_table.num_chars();
                for i in 0..self.cursors.len() {
                    debug_assert!(self.cursors[i].anchor == self.cursors[i].position);
                    if self.cursors[i].position == num_chars.saturating_sub(1)
                        && self.piece_table.char_at(self.cursors[i].position) == Some(b'\n')
                    {
                        continue;
                    } else {
                        content_changes.push(
                            self.delete_chars(
                                self.cursors[i].position,
                                self.cursors[i].position + 1,
                            ),
                        );
                        self.cursors[i].position = min(
                            self.cursors[i].position,
                            self.piece_table.num_chars().saturating_sub(1),
                        );
                    }
                }

                self.lsp_change(content_changes);
                self.syntect_change();
            }
            InsertChar(c) => {
                if self.insertion_stack_dirty {
                    self.insertion_command_stack.clear();
                    self.insertion_stack_dirty = false;
                }
                self.insertion_command_stack.push(InsertChar(c));

                for i in 0..self.cursors.len() {
                    let start = self.cursors[i].position;

                    // Special case for moving over end brackets
                    match c {
                        b')' | b'}' | b']' | b'>' if self.piece_table.char_at(start) == Some(c) => {
                            self.motion(Forward(1));
                            continue;
                        }
                        _ => (),
                    }

                    let changes = self.insert_chars(start, &[c]);
                    self.lsp_change(vec![changes]);

                    // Only show signature help for single cursor
                    if self.cursors.len() == 1 {
                        lsp_signature_help(
                            &mut self.cursors[i],
                            Some(c),
                            &mut self.language_server,
                            &self.piece_table,
                            &self.uri,
                            start + 1,
                        );
                    }

                    lsp_complete(
                        &mut self.cursors[i],
                        Some(c),
                        &mut self.language_server,
                        &self.piece_table,
                        &self.uri,
                        start + 1,
                    );
                    self.cursors[i].position += 1;
                }

                // Special case for inserting brackets
                // Here we don't call InsertChar(c) because we don't want lsp_completion for the closing bracket
                match c {
                    b'(' | b'{' | b'[' | b'<' => {
                        for i in 0..self.cursors.len() {
                            let start = self.cursors[i].position;
                            let changes =
                                self.insert_chars(start, &[text_utils::matching_bracket(c)]);
                            self.lsp_change(vec![changes]);
                        }
                    }
                    _ => (),
                }

                self.syntect_change();
            }
            InsertNewLine => {
                if self.insertion_stack_dirty {
                    self.insertion_command_stack.clear();
                    self.insertion_stack_dirty = false;
                }
                self.insertion_command_stack.push(InsertNewLine);

                let mut content_changes = vec![];

                for cursor in &mut self.cursors {
                    cursor.reset_completion(&mut self.language_server);
                    cursor.reset_signature_help(&mut self.language_server);
                }

                for i in 0..self.cursors.len() {
                    let cursor_position = self.cursors[i].position;

                    let line_indent = self.piece_table.line_indent_width_at_char(cursor_position);
                    let mut chars = vec![b'\n'];
                    chars.append(&mut vec![b' '; line_indent]);

                    let mut cursor_offset = chars.len();

                    if let Some(language) = &self.language {
                        if let Some(indent_chars) = language.indent_chars {
                            if let Some(char_before) =
                                self.piece_table.char_at(cursor_position.saturating_sub(1))
                            {
                                if indent_chars.contains(&char_before) {
                                    chars.append(&mut vec![b' '; self.piece_table.indent_width]);
                                    cursor_offset = chars.len();

                                    let char_after = self.piece_table.char_at(cursor_position);
                                    match (char_before, char_after) {
                                        (b'(', Some(b')'))
                                        | (b'{', Some(b'}'))
                                        | (b'[', Some(b'[')) => {
                                            chars.push(b'\n');
                                            chars.append(&mut vec![b' '; line_indent]);
                                        }
                                        _ => (),
                                    }
                                }
                            }
                        } else if let Some(indent_words) = language.indent_words {
                            for word in indent_words {
                                if self
                                    .piece_table
                                    .line_at_char_starts_with(cursor_position, word.as_bytes())
                                {
                                    chars.append(&mut vec![b' '; self.piece_table.indent_width]);
                                    cursor_offset = chars.len();
                                    break;
                                }
                            }
                        }
                    }

                    content_changes.push(self.insert_chars(cursor_position, &chars));
                    self.cursors[i].position += cursor_offset;
                }

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            IndentLine => {
                let mut content_changes = vec![];

                for i in 0..self.cursors.len() {
                    let line = self.piece_table.line_index(self.cursors[i].position);
                    let anchor_line = self.piece_table.line_index(self.cursors[i].anchor);

                    for i in min(line, anchor_line)..=max(line, anchor_line) {
                        if let Some(line) = self.piece_table.line_at_index(i) {
                            content_changes.push(self.insert_chars(
                                line.start,
                                &vec![b' '; self.piece_table.indent_width],
                            ));
                        }
                    }
                }
                self.motion(ToFirstNonBlankChar);

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            UnindentLine => {
                let mut content_changes = vec![];

                for i in 0..self.cursors.len() {
                    let line = self.piece_table.line_index(self.cursors[i].position);
                    let anchor_line = self.piece_table.line_index(self.cursors[i].anchor);

                    for i in min(line, anchor_line)..=max(line, anchor_line) {
                        if let Some(line) = self.piece_table.line_at_index(i) {
                            if self
                                .piece_table
                                .iter_chars_at(line.start)
                                .take(self.piece_table.indent_width)
                                .all(|c| c == b' ')
                            {
                                let end = line.start + self.piece_table.indent_width;
                                content_changes.push(self.delete_chars(line.start, end));
                            }
                        }
                    }
                }
                self.motion(ToFirstNonBlankChar);

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            // TODO: Improve performance: selecting many lines (1000+) is slow.
            ToggleComment => {
                let line_comment_token = if self.language.is_some() {
                    self.language.unwrap().line_comment_token.unwrap_or("//")
                } else {
                    "//"
                };

                let mut content_changes = vec![];
                let length = line_comment_token.len();
                let mut indent = usize::MAX;
                let mut uncomment = true;

                // We only uncomment if and only if all lines start with a comment
                for i in 0..self.cursors.len() {
                    let line = self.piece_table.line_index(self.cursors[i].position);
                    let anchor_line = self.piece_table.line_index(self.cursors[i].anchor);

                    for i in min(line, anchor_line)..=max(line, anchor_line) {
                        if let Some(line) = self.piece_table.line_at_index(i) {
                            let bytes: Vec<u8> = self
                                .piece_table
                                .iter_chars_at(line.start)
                                .take(line.length)
                                .collect();
                            if bytes.is_empty() {
                                continue;
                            }

                            if !bytes.trim().starts_with_str(line_comment_token.as_bytes()) {
                                uncomment = false;
                            }

                            indent = min(
                                indent,
                                bytes
                                    .iter()
                                    .position(|c| !c.is_ascii_whitespace())
                                    .unwrap_or(0),
                            );
                        }
                    }
                }

                for i in 0..self.cursors.len() {
                    let line = self.piece_table.line_index(self.cursors[i].position);
                    let anchor_line = self.piece_table.line_index(self.cursors[i].anchor);

                    for i in min(line, anchor_line)..=max(line, anchor_line) {
                        if let Some(line) = self.piece_table.line_at_index(i) {
                            let bytes: Vec<u8> = self
                                .piece_table
                                .iter_chars_at(line.start)
                                .take(line.length)
                                .collect();
                            if bytes.is_empty() {
                                continue;
                            }

                            if uncomment {
                                let token_index = bytes.find(line_comment_token).unwrap();
                                let start = line.start + token_index;
                                let end = if bytes
                                    .get(token_index + length)
                                    .is_some_and(|c| c.is_ascii_whitespace())
                                {
                                    start + length + 1
                                } else {
                                    start + length
                                };
                                content_changes.push(self.delete_chars(start, end));
                            } else {
                                let start = line.start + indent;
                                content_changes
                                    .push(self.insert_chars(start, line_comment_token.as_bytes()));
                                content_changes.push(self.insert_chars(start + length, &[b' ']));
                            }
                        }
                    }
                }

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            DeleteCharBack => {
                if self.insertion_stack_dirty {
                    self.insertion_command_stack.clear();
                    self.insertion_stack_dirty = false;
                }
                self.insertion_command_stack.push(DeleteCharBack);

                let mut content_changes = vec![];

                for i in 0..self.cursors.len() {
                    // Special case for deleting bracket pairs (and if at end of file)
                    match (
                        self.piece_table
                            .char_at(self.cursors[i].position.saturating_sub(1)),
                        self.piece_table.char_at(self.cursors[i].position),
                    ) {
                        (Some(b'('), Some(b')'))
                        | (Some(b'{'), Some(b'}'))
                        | (Some(b'['), Some(b']'))
                        | (Some(b'<'), Some(b'>')) => {
                            let start = self.cursors[i].position.saturating_sub(1);
                            let end = self.cursors[i].position + 1;
                            content_changes.push(self.delete_chars(start, end));
                            self.cursors[i].position = start;
                            continue;
                        }
                        _ => (),
                    }

                    let count = self
                        .piece_table
                        .line_at_char(self.cursors[i].position)
                        .and_then(|line| {
                            let num = self.cursors[i].position - line.start;
                            if num > 0
                                && self
                                    .piece_table
                                    .iter_chars_at(line.start)
                                    .take(num)
                                    .all(|c| c == b' ')
                            {
                                let rem = num % self.piece_table.indent_width;
                                if rem == 0 {
                                    Some(self.piece_table.indent_width)
                                } else {
                                    Some(rem)
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or(1);

                    let start = self.cursors[i].position.saturating_sub(count);
                    let end = self.cursors[i].position;
                    content_changes.push(self.delete_chars(start, end));
                    self.cursors[i].position = start;
                }

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            DeleteWordBack => {
                if self.insertion_stack_dirty {
                    self.insertion_command_stack.clear();
                    self.insertion_stack_dirty = false;
                }
                self.insertion_command_stack.push(DeleteWordBack);

                let mut content_changes = vec![];

                for i in 0..self.cursors.len() {
                    if let Some(line) = self.piece_table.line_at_char(self.cursors[i].position) {
                        if self.cursors[i].position == line.start {
                            let start = self.cursors[i].position.saturating_sub(1);
                            let end = self.cursors[i].position;
                            content_changes.push(self.delete_chars(start, end));
                            self.cursors[i].position = start;
                            continue;
                        }

                        if let Some(c) = self
                            .piece_table
                            .char_at(self.cursors[i].position.saturating_sub(1))
                        {
                            let char_type = text_utils::char_type(c);

                            let backward_match = self.cursors[i]
                                .chars_until_pred_rev(&self.piece_table, |c| {
                                    text_utils::char_type(c) != char_type
                                })
                                .unwrap_or(line.length);
                            let start = max(line.start, self.cursors[i].position - backward_match);
                            let end = self.cursors[i].position;
                            content_changes.push(self.delete_chars(start, end));
                            self.cursors[i].position = start;
                        }
                    }
                }

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            DeleteWordFront => {
                if self.insertion_stack_dirty {
                    self.insertion_command_stack.clear();
                    self.insertion_stack_dirty = false;
                }
                self.insertion_command_stack.push(DeleteWordFront);

                let mut content_changes = vec![];

                for i in 0..self.cursors.len() {
                    if let Some(line) = self.piece_table.line_at_char(self.cursors[i].position) {
                        if self.cursors[i].position == line.end {
                            let start = self.cursors[i].position;
                            let end =
                                min(self.cursors[i].position + 1, self.piece_table.num_chars());
                            content_changes.push(self.delete_chars(start, end));
                            self.cursors[i].position = start;
                            continue;
                        }

                        if let Some(c) = self.piece_table.char_at(self.cursors[i].position) {
                            let char_type = text_utils::char_type(c);

                            let forward_match = self.cursors[i]
                                .chars_until_pred(&self.piece_table, |c| {
                                    text_utils::char_type(c) != char_type
                                })
                                .map(|x| x + 1)
                                .unwrap_or(line.length);
                            let start = self.cursors[i].position;
                            let end = min(line.end, self.cursors[i].position + forward_match);
                            content_changes.push(self.delete_chars(start, end));
                            self.cursors[i].position = start;
                        }
                    }
                }

                self.syntect_change();
                self.lsp_change(content_changes);
            }
            Undo => {
                let first_position = self
                    .cursors
                    .iter()
                    .min_by(|x, y| x.position.cmp(&y.position))
                    .map(|cursor| cursor.position)
                    .unwrap_or(0);

                self.clear_diagnostics();
                if let Some(state) = self.undo_stack.pop() {
                    self.redo_stack.push(BufferState {
                        pieces: self.piece_table.pieces.clone(),
                        cursors: self.cursors.clone(),
                    });
                    self.piece_table.pieces = state.pieces;
                    self.cursors = state.cursors;
                }

                let second_position = self
                    .cursors
                    .iter()
                    .min_by(|x, y| x.position.cmp(&y.position))
                    .map(|cursor| cursor.position)
                    .unwrap_or(0);

                self.update_syntect(min(
                    self.piece_table.line_index(first_position),
                    self.piece_table.line_index(second_position),
                ));
                self.lsp_reload();
            }
            Redo => {
                let first_position = self
                    .cursors
                    .iter()
                    .min_by(|x, y| x.position.cmp(&y.position))
                    .map(|cursor| cursor.position)
                    .unwrap_or(0);

                self.clear_diagnostics();
                if let Some(state) = self.redo_stack.pop() {
                    self.undo_stack.push(BufferState {
                        pieces: self.piece_table.pieces.clone(),
                        cursors: self.cursors.clone(),
                    });
                    self.piece_table.pieces = state.pieces;
                    self.cursors = state.cursors;
                }

                let second_position = self
                    .cursors
                    .iter()
                    .min_by(|x, y| x.position.cmp(&y.position))
                    .map(|cursor| cursor.position)
                    .unwrap_or(0);

                self.update_syntect(min(
                    self.piece_table.line_index(first_position),
                    self.piece_table.line_index(second_position),
                ));
                self.lsp_reload();
            }
            StartCompletion => {
                for i in 0..self.cursors.len() {
                    let cursor_position = self.cursors[i].position;

                    let offset = 0;

                    // Only show signature help for single cursor
                    if self.cursors.len() == 1 {
                        lsp_signature_help(
                            &mut self.cursors[i],
                            None,
                            &mut self.language_server,
                            &self.piece_table,
                            &self.uri,
                            cursor_position.saturating_sub(offset),
                        );
                    }

                    lsp_complete(
                        &mut self.cursors[i],
                        None,
                        &mut self.language_server,
                        &self.piece_table,
                        &self.uri,
                        cursor_position.saturating_sub(offset),
                    );
                }
            }
            Complete => {
                let mut content_changes = vec![];

                for i in 0..self.cursors.len() {
                    let cursor_position = self.cursors[i].position;
                    if let Some(ref mut request) = self.cursors[i].completion_request {
                        let item = self.language_server.as_ref().and_then(|server| {
                            server.borrow().saved_completions.get(&request.id).map(
                                |completion_list| {
                                    get_filtered_completions(
                                        &self.piece_table,
                                        completion_list,
                                        request,
                                        cursor_position,
                                    )
                                    .get(request.selection_index)
                                    .cloned()
                                },
                            )
                        });
                        if let Some(item) = item.flatten() {
                            if let Some(text_edit) = item.text_edit {
                                let start = self
                                    .piece_table
                                    .char_index_from_line_col(
                                        text_edit.range.start.line as usize,
                                        text_edit.range.start.character as usize,
                                    )
                                    .unwrap_or(cursor_position);

                                // The end of the completion is the original text edit range
                                // plus the difference in cursor position
                                // (from when the completion was triggered until now)
                                let end = self
                                    .piece_table
                                    .char_index_from_line_col(
                                        text_edit.range.end.line as usize,
                                        text_edit.range.end.character as usize,
                                    )
                                    .unwrap_or(cursor_position)
                                    + (cursor_position.saturating_sub(request.position));

                                content_changes.push(self.delete_chars(start, end));
                                self.cursors[i].position = start;

                                content_changes
                                    .push(self.insert_chars(start, text_edit.new_text.as_bytes()));
                                self.cursors[i].position += text_edit.new_text.len();
                                self.cursors[i].reset_completion(&mut self.language_server);
                            }
                        }
                    }
                }

                self.syntect_change();
                self.lsp_change(content_changes)
            }
            CopySelection => {
                let num_cursors = self.cursors.len();
                let mut selection: Vec<u8> = vec![];
                for cursor in &mut self.cursors {
                    cursor.save_selection_to_clipboard(&self.piece_table);
                    selection.extend(cursor.get_selection(&self.piece_table));

                    // Insert new lines between the concatenated clipboard content in multi-cursor mode
                    if num_cursors > 1 {
                        selection.push(b'\n');
                    }
                }
                self.platform_resources.set_clipboard(&selection);
            }
            CopyLine => {
                // Save positions
                let mut cursor_positions = vec![];
                for cursor in &self.cursors {
                    cursor_positions.push((cursor.anchor, cursor.position));
                }

                self.motion(ExtendSelection);
                self.command(CopySelection);

                // Restore positions
                for (i, cursor) in self.cursors.iter_mut().enumerate() {
                    let (anchor, position) = cursor_positions[i];
                    cursor.anchor = anchor;
                    cursor.position = position;
                }
            }
            PasteSelection => {
                for i in 0..self.cursors.len() {
                    let text = self.platform_resources.get_clipboard();
                    let num_chars = self.piece_table.num_chars();
                    let (start, count) = if text.last().is_some_and(|c| *c == b'\n') {
                        (
                            self.piece_table
                                .line_at_char(self.cursors[i].position)
                                .map(|line| min(line.end + 1, num_chars))
                                .unwrap_or(num_chars),
                            text.len() - text.as_bstr().trim_ascii_start().len(),
                        )
                    } else {
                        (min(self.cursors[i].position + 1, num_chars), text.len())
                    };

                    let changes = self.insert_chars(start, &text);
                    self.lsp_change(vec![changes]);
                    self.syntect_change();
                    self.cursors[i].position = start + count;
                }
            }
            PasteCursorSelection => {
                for i in 0..self.cursors.len() {
                    let start = min(self.cursors[i].position + 1, self.piece_table.num_chars());
                    let text = self.cursors[i].clipboard;
                    let size = self.cursors[i].clipboard_size;

                    let changes = self.insert_chars(start, &text[0..size]);
                    self.lsp_change(vec![changes]);
                    self.syntect_change();
                    self.cursors[i].position += size;
                }
            }
            GotoDefinition => {
                if let Some(last_cursor) = self.cursors.last() {
                    self.lsp_goto_definition(last_cursor.position);
                }
            }
            GotoImplementation => {
                if let Some(last_cursor) = self.cursors.last() {
                    self.lsp_goto_implementation(last_cursor.position);
                }
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
                cursor.reset_completion(&mut self.language_server);
            }
            if self.mode == Insert
                && cursor
                    .signature_help_request
                    .is_some_and(|request| request.position > cursor.position)
            {
                cursor.reset_signature_help(&mut self.language_server);
            }

            if self.mode == Insert || self.mode == Normal {
                cursor.reset_anchor();
            }

            cursor.unstick_col(&self.piece_table);
            cursor.reset_completion_view(&mut self.language_server);
        }
    }

    fn delete_chars(&mut self, start: usize, end: usize) -> TextDocumentChangeEvent {
        let old_diagnostic_positions = self.diagnostic_positions();
        let (line1, col1) = (
            self.piece_table.line_index(start),
            self.piece_table.col_index(start),
        );
        let (line2, col2) = (
            self.piece_table.line_index(end),
            self.piece_table.col_index(end),
        );
        self.piece_table.delete(start, end);
        self.delete_rebalance(start, end, &old_diagnostic_positions);
        TextDocumentChangeEvent {
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
        }
    }

    fn insert_chars(&mut self, start: usize, text: &[u8]) -> TextDocumentChangeEvent {
        let old_diagnostic_positions = self.diagnostic_positions();
        self.piece_table.insert(start, text);
        let (line, col) = (
            self.piece_table.line_index(start),
            self.piece_table.col_index(start),
        );
        self.insert_rebalance(start, text.len(), &old_diagnostic_positions);
        TextDocumentChangeEvent {
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
            text: text.as_bstr().to_string(),
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

            cursor.reset_completion(&mut self.language_server);
            cursor.reset_signature_help(&mut self.language_server);
            cursor.reset_anchor();
            cursor.unstick_col(&self.piece_table);
        }
    }

    fn switch_to_insert_mode(&mut self) {
        self.mode = Insert;
        self.insertion_stack_dirty = true;
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

    fn syntect_change(&mut self) {
        let first_line = self
            .cursors
            .iter()
            .map(|x| self.piece_table.line_index(x.position))
            .min()
            .unwrap_or(0);
        self.update_syntect(first_line);
    }

    fn lsp_reload(&mut self) {
        if let Some(server) = &self.language_server {
            let change_params = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: self.uri.to_string(),
                    version: self.version,
                },
                content_changes: vec![TextDocumentChangeEvent {
                    range: None,
                    text: unsafe {
                        String::from_utf8_unchecked(self.piece_table.iter_chars().collect())
                    },
                }],
            };
            server
                .borrow_mut()
                .send_notification("textDocument/didChange", change_params);
            self.version += 1;
        }
    }

    fn lsp_change(&mut self, content_changes: Vec<TextDocumentChangeEvent>) {
        if let Some(server) = &self.language_server {
            let change_params = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: self.uri.to_string(),
                    version: self.version,
                },
                content_changes,
            };
            server
                .borrow_mut()
                .send_notification("textDocument/didChange", change_params);
            self.version += 1;
        }
    }

    fn lsp_goto_definition(&mut self, position: usize) {
        if let Some(server) = &self.language_server {
            let (line, col) = (
                self.piece_table.line_index(position),
                self.piece_table.col_index(position),
            );
            let definition_params = DefinitionParams {
                text_document: TextDocumentIdentifier {
                    uri: self.uri.to_string(),
                },
                position: Position {
                    line: line as u32,
                    character: col as u32,
                },
            };
            server
                .borrow_mut()
                .send_request("textDocument/definition", definition_params);
        }
    }

    fn lsp_goto_implementation(&mut self, position: usize) {
        if let Some(server) = &self.language_server {
            let (line, col) = (
                self.piece_table.line_index(position),
                self.piece_table.col_index(position),
            );
            let definition_params = ImplementationParams {
                text_document: TextDocumentIdentifier {
                    uri: self.uri.to_string(),
                },
                position: Position {
                    line: line as u32,
                    character: col as u32,
                },
            };
            server
                .borrow_mut()
                .send_request("textDocument/implementation", definition_params);
        }
    }

    fn lsp_hover(&mut self, line: usize, col: usize) {
        if let Some(server) = &self.language_server {
            let hover_params = HoverParams {
                text_document: TextDocumentIdentifier {
                    uri: self.uri.to_string(),
                },
                position: Position {
                    line: line as u32,
                    character: col as u32,
                },
            };
            server
                .borrow_mut()
                .send_request("textDocument/hover", hover_params);
        }
    }

    fn insert_rebalance(
        &mut self,
        position: usize,
        count: usize,
        old_diagnostic_positions: &Option<Vec<(usize, usize)>>,
    ) {
        cursors_insert_rebalance(&mut self.cursors, position, count);
        self.syntect_insert_rebalance(position, count);
        if let Some(positions) = old_diagnostic_positions {
            self.diagnostics_insert_rebalance(position, count, positions);
        }
    }

    fn delete_rebalance(
        &mut self,
        position: usize,
        end: usize,
        old_diagnostic_positions: &Option<Vec<(usize, usize)>>,
    ) {
        cursors_delete_rebalance(&mut self.cursors, position, end);
        self.syntect_delete_rebalance(position, end);
        if let Some(positions) = old_diagnostic_positions {
            self.diagnostics_delete_rebalance(position, end, positions);
        }
    }

    fn syntect_delete_rebalance(&mut self, position: usize, end: usize) {
        if let Some(syntect) = &mut self.syntect {
            syntect.delete_rebalance(&self.piece_table, position, end);
        }
    }

    fn syntect_insert_rebalance(&mut self, position: usize, count: usize) {
        if let Some(syntect) = &mut self.syntect {
            syntect.insert_rebalance(&self.piece_table, position, count);
        }
    }

    fn diagnostic_positions(&self) -> Option<Vec<(usize, usize)>> {
        if let Some(server) = &self.language_server {
            if let Some(diagnostics) = server
                .borrow()
                .saved_diagnostics
                .get(&self.uri.to_lowercase())
            {
                let mut positions = vec![];
                for diagnostic in diagnostics {
                    if let (Some(start), Some(end)) = (
                        self.piece_table.char_index_from_line_col(
                            diagnostic.range.start.line as usize,
                            diagnostic.range.start.character as usize,
                        ),
                        self.piece_table.char_index_from_line_col(
                            diagnostic.range.end.line as usize,
                            diagnostic.range.end.character as usize,
                        ),
                    ) {
                        positions.push((start, end));
                    } else {
                        positions.push((0, 0));
                    }
                }
                if !positions.is_empty() {
                    return Some(positions);
                }
            }
        }
        None
    }

    fn diagnostics_insert_rebalance(
        &mut self,
        position: usize,
        count: usize,
        old_positions: &[(usize, usize)],
    ) {
        if let Some(server) = &self.language_server {
            if let Some(diagnostics) = server
                .borrow_mut()
                .saved_diagnostics
                .get_mut(&self.uri.to_lowercase())
            {
                for i in 0..diagnostics.len() {
                    let (mut start, mut end) = old_positions[i];
                    if start > position {
                        start += count;
                    }
                    if end > position {
                        end += count;
                    }
                    diagnostics[i].range.start.line = self.piece_table.line_index(start) as u32;
                    diagnostics[i].range.start.character = self.piece_table.col_index(start) as u32;
                    diagnostics[i].range.end.line = self.piece_table.line_index(end) as u32;
                    diagnostics[i].range.end.character = self.piece_table.col_index(end) as u32;
                }
            }
        }
    }

    fn diagnostics_delete_rebalance(
        &mut self,
        position: usize,
        end: usize,
        old_positions: &[(usize, usize)],
    ) {
        let count = end - position;
        if let Some(server) = &self.language_server {
            if let Some(diagnostics) = server
                .borrow_mut()
                .saved_diagnostics
                .get_mut(&self.uri.to_lowercase())
            {
                for i in 0..diagnostics.len() {
                    let (mut start, mut end) = old_positions[i];
                    if start >= position {
                        start = start.saturating_sub(count);
                    }
                    if end >= position {
                        end = end.saturating_sub(count);
                    }
                    diagnostics[i].range.start.line = self.piece_table.line_index(start) as u32;
                    diagnostics[i].range.start.character = self.piece_table.col_index(start) as u32;
                    diagnostics[i].range.end.line = self.piece_table.line_index(end) as u32;
                    diagnostics[i].range.end.character = self.piece_table.col_index(end) as u32;
                }
            }
        }
    }

    fn clear_diagnostics(&mut self) {
        if let Some(server) = &self.language_server {
            server
                .borrow_mut()
                .saved_diagnostics
                .remove(&self.uri.to_lowercase());
        }
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

        let is_trigger_character =
            character.is_some_and(|c| server.borrow().trigger_characters.contains(&c));

        if cursor.completion_request.is_some() && !is_trigger_character {
            let request = cursor.completion_request.as_mut().unwrap();
            if server
                .borrow()
                .saved_completions
                .get(&request.id)
                .is_some_and(|request| request.is_incomplete)
            {
                if let Some(id) = server
                    .borrow_mut()
                    .send_request("textDocument/completion", completion_params)
                {
                    request.next_id = Some(id);
                    request.next_position = Some(position);
                }
            }
        } else if character.is_none() || is_trigger_character {
            if let Some(id) = server
                .borrow_mut()
                .send_request("textDocument/completion", completion_params)
            {
                cursor.completion_request = Some(CompletionRequest {
                    id,
                    next_id: None,
                    position,
                    next_position: None,
                    initial_position: position,
                    selection_index: 0,
                    selection_view_offset: 0,
                    manually_triggered: character.is_none(),
                });
            }
        }
    }
}

fn lsp_signature_help(
    cursor: &mut Cursor,
    character: Option<u8>,
    language_server: &mut Option<Rc<RefCell<LanguageServer>>>,
    piece_table: &PieceTable,
    uri: &str,
    position: usize,
) {
    if let Some(server) = &language_server {
        if character.is_some_and(|c| {
            server
                .borrow()
                .signature_help_trigger_characters
                .contains(&c)
        }) {
            let (line, col) = (
                piece_table.line_index(position),
                piece_table.col_index(position),
            );
            let signature_help_params = SignatureHelpParams {
                text_document: TextDocumentIdentifier {
                    uri: uri.to_string(),
                },
                position: Position {
                    line: line as u32,
                    character: col as u32,
                },
                context: SignatureHelpContext {
                    trigger_kind: if character.is_none() { 1 } else { 2 },
                    trigger_character: character.map(|c| c.to_string()),
                    is_retrigger: cursor.signature_help_request.is_some(),
                    active_signature_help: cursor.signature_help_request.and_then(|request| {
                        server
                            .borrow()
                            .saved_signature_helps
                            .get(&request.id)
                            .cloned()
                    }),
                },
            };
            if let Some(id) = server
                .borrow_mut()
                .send_request("textDocument/signatureHelp", signature_help_params)
            {
                if let Some(request) = cursor.signature_help_request.as_mut() {
                    request.next_id = Some(id);
                    request.next_position = Some(position);
                } else {
                    cursor.signature_help_request = Some(SignatureHelpRequest {
                        id,
                        next_id: None,
                        position,
                        next_position: None,
                    });
                }
            }
        }
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
                || (str.starts_with("ci") && str.len() <= 3)
                || (str.starts_with("di") && str.len() <= 3)
                || (str.starts_with("ct") && str.len() <= 3)
                || (str.starts_with("dt") && str.len() <= 3)
                || (str.starts_with("cT") && str.len() <= 3)
                || (str.starts_with("dT") && str.len() <= 3)
        }
        BufferMode::Visual => {
            VISUAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
                || (str.starts_with('f') && str.len() <= 2)
                || (str.starts_with('F') && str.len() <= 2)
                || (str.starts_with('t') && str.len() <= 2)
                || (str.starts_with('T') && str.len() <= 2)
                || (str.starts_with('i') && str.len() <= 2)
        }
        BufferMode::VisualLine => {
            VISUAL_MODE_COMMANDS.iter().any(|cmd| str.is_prefix_of(cmd))
                || (str.starts_with('f') && str.len() <= 2)
                || (str.starts_with('F') && str.len() <= 2)
                || (str.starts_with('t') && str.len() <= 2)
                || (str.starts_with('T') && str.len() <= 2)
        }
        _ => false,
    }
}

const NORMAL_MODE_COMMANDS: [&str; 30] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "dd", "D", "J", "K", "v", "V", "u",
    ">", "<", "p", "P", "yy", "zz", "n", "N", "/", "gd", "gi", ".",
];
const VISUAL_MODE_COMMANDS: [&str; 21] = [
    "j", "k", "h", "l", "w", "b", "^", "$", "gg", "G", "x", "d", ">", "<", "y", "p", "P", "zz",
    "n", "N", "/",
];

#[derive(Clone, Copy, PartialEq)]
enum CutMotion {
    Inside,
    ForwardUntil,
    ForwardTo,
    BackwardUntil,
    BackwardTo,
}

enum CursorMotion<'a> {
    Forward(usize),
    Backward(usize),
    BackwardOnceWrapping,
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
    ForwardUntilChar(u8),
    BackwardUntilChar(u8),
    ExtendSelection,
    ExtendSelectionInside(u8),
    GotoLine(usize),
    SeekUntil(&'a [u8]),
    SeekBackUntil(&'a [u8]),
    SeekToSelf(&'a [u8]),
    SeekBackToSelf(&'a [u8]),
}

#[derive(Clone, Copy, PartialEq)]
enum BufferCommand {
    InsertCursorAbove,
    InsertCursorBelow,
    ReplaceChar(u8),
    CutSelection,
    CutSingleSelection,
    CutMotion(u8, CutMotion, bool),
    InsertChar(u8),
    InsertNewLine,
    IndentLine,
    UnindentLine,
    ToggleComment,
    DeleteCharBack,
    DeleteWordBack,
    DeleteWordFront,
    Undo,
    Redo,
    StartCompletion,
    Complete,
    CopySelection,
    CopyLine,
    PasteSelection,
    PasteCursorSelection,
    GotoDefinition,
    GotoImplementation,
}
