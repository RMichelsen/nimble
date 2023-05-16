use std::{
    cell::RefCell,
    cmp::min,
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::{BufRead, BufReader},
    rc::Rc,
};

use fuzzy_matcher::{clangd::ClangdMatcher, FuzzyMatcher};
use walkdir::WalkDir;
use winit::{
    dpi::LogicalPosition,
    event::{ModifiersState, VirtualKeyCode},
    window::Window,
};

use crate::{
    buffer::Buffer,
    language_server::LanguageServer,
    language_server_types::VoidParams,
    language_support::{
        language_from_path, CPP_FILE_EXTENSIONS, PYTHON_FILE_EXTENSIONS, RUST_FILE_EXTENSIONS,
    },
    platform_resources,
    renderer::{RenderLayout, Renderer},
    view::View,
};

struct Document {
    buffer: Buffer,
    view: View,
}

pub const MAX_SHOWN_FILE_FINDER_ITEMS: usize = 10;

pub enum EditorCommand {
    CenterView,
    CenterIfNotVisible,
    Quit,
    QuitAll,
    QuitNoCheck,
    QuitAllNoCheck,
}

#[derive(Debug)]
pub struct FileIdentifier {
    pub name: OsString,
    pub path: OsString,
}

pub struct FileFinder {
    pub files: Vec<FileIdentifier>,
    pub search_string: String,
    pub selection_index: usize,
    pub selection_view_offset: usize,
}

pub struct Workspace {
    pub path: String,
    pub gitignore_paths: Vec<String>,
}

pub struct Editor {
    renderer: Renderer,
    workspace: Option<Workspace>,
    file_finder: Option<FileFinder>,
    documents: HashMap<String, Document>,
    active_document: Option<String>,
    active_document_layout: RenderLayout,
    numbers_layout: RenderLayout,
    file_finder_layout: RenderLayout,
    status_line_layout: RenderLayout,
    language_servers: HashMap<&'static str, Rc<RefCell<LanguageServer>>>,
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            renderer: Renderer::new(window),
            workspace: None,
            file_finder: None,
            documents: HashMap::default(),
            active_document: None,
            active_document_layout: RenderLayout::default(),
            numbers_layout: RenderLayout::default(),
            file_finder_layout: RenderLayout::default(),
            status_line_layout: RenderLayout::default(),
            language_servers: HashMap::default(),
        }
    }

    pub fn update_highlights(&mut self) -> bool {
        if let Some(document) = &mut self.active_document {
            if let Some(document) = self.documents.get_mut(document) {
                return document.buffer.update_highlights();
            }
        }
        false
    }

    pub fn update_layouts(&mut self, window: &Window) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let font_size = self.renderer.get_font_size();

        if let Some(document) = &self.active_document {
            let document = &self.documents[document];

            let numbers_num_cols = (0..)
                .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                .count()
                + 2;

            self.active_document_layout = RenderLayout {
                row_offset: 0,
                col_offset: numbers_num_cols,
                num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                num_cols: (window_size.0 / font_size.0).ceil() as usize,
            };

            self.numbers_layout = RenderLayout {
                row_offset: 0,
                col_offset: 0,
                num_rows: self.active_document_layout.num_rows,
                num_cols: numbers_num_cols.saturating_sub(2),
            };
        }

        self.status_line_layout = RenderLayout {
            row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
            col_offset: 0,
            num_rows: 2,
            num_cols: (window_size.0 / font_size.0).ceil() as usize,
        };

        if let (Some(workspace), Some(file_finder)) = (&self.workspace, &self.file_finder) {
            let num_cols = (window_size.0 / font_size.0).ceil() as usize;
            self.file_finder_layout = RenderLayout {
                row_offset: 0,
                col_offset: num_cols / 2,
                num_rows: (window_size.1 / font_size.1).ceil() as usize,
                num_cols,
            };
        }
    }

    pub fn open_workspace(&mut self) -> bool {
        if let Some(path) = platform_resources::open_folder() {
            self.workspace = Some(Workspace::new(&path));
            return true;
        }
        false
    }

    pub fn handle_lsp_responses(&mut self) -> bool {
        let mut require_redraw = false;

        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            match server.handle_responses() {
                Ok((responses, notifications)) => {
                    for response in responses {
                        match response.method {
                            "initialize" => {
                                for document in self.documents.values() {
                                    if let Some(language) = document.buffer.language {
                                        if *identifier == language.identifier {
                                            document.buffer.send_did_open(&mut server);
                                        }
                                    }
                                }
                            }
                            "textDocument/completion" => {
                                if let Some(value) = response.value {
                                    server.save_completions(response.id, value);
                                }
                                if let Some(document) = &self.active_document {
                                    self.documents
                                        .get_mut(document)
                                        .unwrap()
                                        .buffer
                                        .update_completions(&mut server);
                                }
                                require_redraw = true;
                            }
                            "textDocument/signatureHelp" => {
                                if let Some(value) = response.value {
                                    server.save_signature_help(response.id, value);
                                }
                                if let Some(document) = &self.active_document {
                                    self.documents
                                        .get_mut(document)
                                        .unwrap()
                                        .buffer
                                        .update_signature_helps(&mut server);
                                }
                                require_redraw = true;
                            }
                            _ => (),
                        }
                    }
                    for notification in notifications {
                        if notification.method.as_str() == "textDocument/publishDiagnostics" {
                            if let Some(value) = notification.value {
                                server.save_diagnostics(value);
                            }
                            require_redraw = true;
                        }
                    }
                }
                Err(e) => {
                    todo!();
                }
            }
        }

        require_redraw
    }

    pub fn render(&mut self, window: &Window) {
        self.renderer.start_draw();

        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        if let Some(document) = &self.active_document {
            let document = &self.documents[document];

            self.renderer.draw_buffer(
                &document.buffer,
                &active_document_layout,
                &document.view,
                &document.buffer.language_server,
            );

            self.renderer
                .draw_numbers(&document.buffer, &self.numbers_layout, &document.view);
        }

        if let (Some(workspace), Some(file_finder)) = (&self.workspace, &self.file_finder) {
            self.renderer.draw_file_finder(
                &mut self.file_finder_layout,
                &workspace.path,
                file_finder,
            );
        }

        self.renderer.draw_status_line(
            &self.workspace,
            &self.active_document,
            &self.status_line_layout,
        );

        self.renderer.end_draw();
    }

    pub fn lsp_shutdown(&mut self) {
        for (identifier, server) in &mut self.language_servers {
            let mut server = server.borrow_mut();
            // According to the spec clients should wait for LSP response,
            // but we don't have time for that..
            server.send_request("shutdown", VoidParams {});
            std::thread::sleep(std::time::Duration::from_millis(10));
            server.send_notification("exit", VoidParams {});
        }
    }

    pub fn handle_mouse_pressed(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
    ) {
        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) =
                document
                    .view
                    .get_line_col(&active_document_layout, mouse_position, font_size);
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                document.buffer.insert_cursor(line, col);
            } else {
                document.buffer.set_cursor(line, col);
            }
        }
    }

    pub fn handle_mouse_drag(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
    ) {
        if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
            return;
        }

        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) =
                document
                    .view
                    .get_line_col(&active_document_layout, mouse_position, font_size);
            document.buffer.set_drag(line, col);
        }
    }

    pub fn handle_mouse_double_click(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
    ) -> bool {
        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) =
                document
                    .view
                    .get_line_col(&active_document_layout, mouse_position, font_size);
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                document.buffer.insert_cursor(line, col);
            } else if document.buffer.handle_mouse_double_click(line, col) {
                return true;
            }
        }
        false
    }

    pub fn handle_scroll(&mut self, sign: isize) {
        if let Some(document) = self.active_document() {
            document.view.handle_scroll(&document.buffer, sign);
        }
    }

    pub fn handle_mouse_hover(&mut self, mouse_position: LogicalPosition<f64>) {
        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            document
                .view
                .hover(&active_document_layout, mouse_position, font_size);
        }
    }

    pub fn handle_mouse_exit_hover(&mut self) {
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            document.view.exit_hover();
        }
    }

    pub fn hovering(&mut self) -> bool {
        if let Some(document) = self.active_document() {
            return document.view.hover.is_some();
        }
        false
    }

    pub fn has_moved_cell(
        &mut self,
        cached_mouse_position: LogicalPosition<f64>,
        mouse_position: LogicalPosition<f64>,
    ) -> bool {
        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        if let Some(document) = self.active_document() {
            let (line, col) =
                document
                    .view
                    .get_line_col(&active_document_layout, mouse_position, font_size);
            return (line, col)
                != document.view.get_line_col(
                    &active_document_layout,
                    cached_mouse_position,
                    font_size,
                );
        }
        false
    }

    pub fn handle_key(
        &mut self,
        window: &Window,
        key_code: VirtualKeyCode,
        modifiers: Option<ModifiersState>,
    ) -> bool {
        match key_code {
            VirtualKeyCode::T if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.renderer.cycle_theme();
                return true;
            }
            VirtualKeyCode::O if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if self.ready_to_quit() && self.open_workspace() {
                    self.documents.clear();
                    self.active_document = None;
                    self.lsp_shutdown();
                    self.language_servers.clear();
                }

                return true;
            }
            VirtualKeyCode::P
                if self.workspace.is_some()
                    && modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) =>
            {
                self.file_finder = Some(FileFinder::new(self.workspace.as_ref().unwrap()));
                return true;
            }
            VirtualKeyCode::J if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if let Some(file_finder) = &mut self.file_finder {
                    let num_shown_file_finder_items =
                        min(file_finder.files.len(), MAX_SHOWN_FILE_FINDER_ITEMS);
                    file_finder.selection_index = min(
                        file_finder.selection_index + 1,
                        file_finder.files.len().saturating_sub(1),
                    );
                    if file_finder.selection_index
                        >= file_finder.selection_view_offset + num_shown_file_finder_items
                    {
                        file_finder.selection_view_offset += 1;
                    }
                    return true;
                }
            }
            VirtualKeyCode::K if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if let Some(file_finder) = &mut self.file_finder {
                    file_finder.selection_index = file_finder.selection_index.saturating_sub(1);
                    if file_finder.selection_index < file_finder.selection_view_offset {
                        file_finder.selection_view_offset -= 1;
                    }
                    return true;
                }
            }
            VirtualKeyCode::Back if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if let Some(file_finder) = &mut self.file_finder {
                    file_finder.search_string.clear();
                    return true;
                }
            }
            VirtualKeyCode::Back => {
                if let Some(file_finder) = &mut self.file_finder {
                    file_finder.search_string.pop();
                    file_finder.filter_files();
                    return true;
                }
            }
            VirtualKeyCode::Return => {
                if let Some(file_finder) = &mut self.file_finder {
                    if let Some(path) = file_finder.files[file_finder.selection_index]
                        .path
                        .clone()
                        .to_str()
                    {
                        self.open_file(path, window);
                    }

                    self.file_finder = None;
                    return true;
                }
            }
            VirtualKeyCode::Escape => {
                if let Some(file_finder) = &mut self.file_finder {
                    self.file_finder = None;
                    return true;
                }
            }
            _ if self.file_finder.is_some() => return true,
            _ => (),
        }

        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();
        let mut delayed_command = None;
        if let Some(document) = self.active_document() {
            if let Some(editor_command) = document.buffer.handle_key(
                key_code,
                modifiers,
                &document.view,
                active_document_layout.num_rows,
                active_document_layout.num_cols,
            ) {
                match editor_command {
                    EditorCommand::CenterView => document
                        .view
                        .center(&document.buffer, &active_document_layout),
                    EditorCommand::CenterIfNotVisible => document
                        .view
                        .center_if_not_visible(&document.buffer, &active_document_layout),
                    x => delayed_command = Some(x),
                }
                document
                    .view
                    .adjust(&document.buffer, &active_document_layout)
            }
        }

        if let Some(command) = delayed_command {
            match command {
                EditorCommand::Quit => {
                    let ready_to_quit = self
                        .active_document()
                        .is_some_and(|document| document.buffer.ready_to_quit());

                    if ready_to_quit {
                        self.documents
                            .remove(self.active_document.as_ref().unwrap().as_str());
                        self.active_document = self
                            .documents
                            .iter()
                            .last()
                            .map(|document| document.0.clone());
                    }

                    return !self.documents.is_empty();
                }
                EditorCommand::QuitNoCheck => {
                    self.documents
                        .remove(self.active_document.as_ref().unwrap().as_str());
                    self.active_document = self
                        .documents
                        .iter()
                        .last()
                        .map(|document| document.0.clone());

                    return !self.documents.is_empty();
                }
                EditorCommand::QuitAll => {
                    let ready_to_quit = self.ready_to_quit();
                    if ready_to_quit {
                        self.documents.clear();
                        self.active_document = None;
                        return false;
                    }
                }
                EditorCommand::QuitAllNoCheck => {
                    self.documents.clear();
                    self.active_document = None;
                    return false;
                }
                _ => (),
            }
        }

        true
    }

    pub fn handle_char(&mut self, window: &Window, c: char) -> bool {
        if let Some(file_finder) = &mut self.file_finder {
            if c as u8 >= 0x20 && c as u8 <= 0x7E {
                file_finder.search_string.push(c);
                file_finder.filter_files();
            }
            return true;
        }

        let active_document_layout = self.active_document_layout;
        let font_size = self.renderer.get_font_size();

        let mut delayed_command = None;
        if let Some(document) = self.active_document() {
            if let Some(editor_command) = document.buffer.handle_char(c) {
                match editor_command {
                    EditorCommand::CenterView => document
                        .view
                        .center(&document.buffer, &active_document_layout),
                    EditorCommand::CenterIfNotVisible => document
                        .view
                        .center_if_not_visible(&document.buffer, &active_document_layout),
                    x => delayed_command = Some(x),
                }
            }
            document
                .view
                .adjust(&document.buffer, &active_document_layout)
        }

        if let Some(command) = delayed_command {
            match command {
                EditorCommand::Quit => {
                    let ready_to_quit = self
                        .active_document()
                        .is_some_and(|document| document.buffer.ready_to_quit());

                    if ready_to_quit {
                        self.documents
                            .remove(self.active_document.as_ref().unwrap().as_str());
                        self.active_document = self
                            .documents
                            .iter()
                            .last()
                            .map(|document| document.0.clone());
                    }

                    return !self.documents.is_empty();
                }
                EditorCommand::QuitNoCheck => {
                    self.documents
                        .remove(self.active_document.as_ref().unwrap().as_str());
                    self.active_document = self
                        .documents
                        .iter()
                        .last()
                        .map(|document| document.0.clone());

                    return !self.documents.is_empty();
                }
                EditorCommand::QuitAll => {
                    let ready_to_quit = self.ready_to_quit();
                    self.documents.clear();
                    self.active_document = None;
                    return false;
                }
                EditorCommand::QuitAllNoCheck => {
                    self.documents.clear();
                    self.active_document = None;
                    return false;
                }
                _ => (),
            }
        }

        true
    }

    pub fn ready_to_quit(&mut self) -> bool {
        self.documents
            .iter_mut()
            .all(|(_, document)| document.buffer.ready_to_quit())
    }

    pub fn open_file(&mut self, path: &str, window: &Window) {
        let language_server = language_from_path(path).and_then(|language| {
            LanguageServer::new(language).map(|server| {
                if !self.language_servers.contains_key(language.identifier) {
                    self.language_servers
                        .insert(language.identifier, Rc::new(RefCell::new(server)));
                }
                Rc::clone(self.language_servers.get(language.identifier).unwrap())
            })
        });

        if self.documents.contains_key(path) {
            self.active_document = Some(path.to_string());
        } else {
            self.documents.insert(
                path.to_string(),
                Document {
                    buffer: Buffer::new(window, path, language_server),
                    view: View::new(),
                },
            );
            self.active_document = Some(path.to_string());
        }
    }

    fn active_document(&mut self) -> Option<&mut Document> {
        if let Some(document) = &self.active_document {
            self.documents.get_mut(document)
        } else {
            None
        }
    }
}

impl Workspace {
    pub fn new(path: &str) -> Self {
        let gitignore_paths = if let Ok(gitignore) = File::open(path.to_string() + "/.gitignore") {
            BufReader::new(gitignore)
                .lines()
                .flatten()
                .map(|entry| entry.trim_start_matches('/').to_string())
                .map(|entry| entry.trim_start_matches('\\').to_string())
                .collect()
        } else {
            vec![]
        };

        Self {
            path: path.to_string(),
            gitignore_paths,
        }
    }
}

impl FileFinder {
    pub fn new(workspace: &Workspace) -> Self {
        Self {
            files: WalkDir::new(&workspace.path)
                .into_iter()
                .filter_entry(|e| {
                    e.file_name() != OsStr::new(".git")
                        && !workspace
                            .gitignore_paths
                            .iter()
                            .any(|entry| entry == e.file_name().to_str().unwrap())
                })
                .flatten()
                .filter(|e| {
                    e.file_type().is_file()
                        && e.path().extension().is_some_and(|extension| {
                            let extension = extension.to_str().unwrap();
                            CPP_FILE_EXTENSIONS.contains(&extension)
                                || RUST_FILE_EXTENSIONS.contains(&extension)
                                || PYTHON_FILE_EXTENSIONS.contains(&extension)
                                || extension == "txt"
                        })
                })
                .map(|e| FileIdentifier {
                    name: e.file_name().to_os_string(),
                    path: e.path().as_os_str().to_os_string(),
                })
                .take(1000)
                .collect(),
            search_string: String::default(),
            selection_index: 0,
            selection_view_offset: 0,
        }
    }

    pub fn filter_files(&mut self) {
        let matcher = ClangdMatcher::default();

        self.files.sort_by(|f0, f1| {
            if let (Some(n0), Some(n1)) = (f0.name.to_str(), f1.name.to_str()) {
                let s0 = matcher.fuzzy_match(n0, &self.search_string).unwrap_or(0);
                let s1 = matcher.fuzzy_match(n1, &self.search_string).unwrap_or(0);
                return s1.cmp(&s0);
            }
            0.cmp(&0)
        });
    }
}
