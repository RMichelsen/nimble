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

pub const MAX_SHOWN_FILE_FINDER_ITEMS: usize = 10;

pub enum EditorCommand {
    CenterView,
    CenterIfNotVisible,
    ToggleSplitView,
    Quit,
    QuitAll,
    QuitNoCheck,
    QuitAllNoCheck,
}

struct Document {
    path: String,
    buffer: Buffer,
    view: View,
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

#[derive(Default, Debug)]
struct DocumentLayout {
    pub layout: RenderLayout,
    pub numbers_layout: RenderLayout,
    pub status_line_layout: RenderLayout,
}

pub struct Editor {
    renderer: Renderer,
    workspace: Option<Workspace>,
    file_finder: Option<FileFinder>,
    open_documents: Vec<Document>,
    active_view: usize,
    split_view: bool,
    visible_documents: [Option<usize>; 2],
    visible_documents_layouts: [DocumentLayout; 2],
    file_finder_layout: RenderLayout,
    language_servers: HashMap<&'static str, Rc<RefCell<LanguageServer>>>,
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            renderer: Renderer::new(window),
            workspace: None,
            file_finder: None,
            open_documents: vec![],
            active_view: 0,
            split_view: false,
            visible_documents: [None, None],
            visible_documents_layouts: [DocumentLayout::default(), DocumentLayout::default()],
            file_finder_layout: RenderLayout::default(),
            language_servers: HashMap::default(),
        }
    }

    pub fn update_highlights(&mut self) -> bool {
        if let Some(i) = self.visible_documents[self.active_view] {
            return self.open_documents[i].buffer.update_highlights();
        }
        false
    }

    pub fn update_layouts(&mut self, window: &Window) {
        self.renderer.ensure_size(window);

        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let font_size = self.renderer.get_font_size();

        self.visible_documents_layouts = match self.visible_documents {
            [Some(i), Some(j)] if self.split_view => {
                let left_document = &mut self.open_documents[i];
                let left_numbers_num_cols = (0..)
                    .take_while(|i| 10usize.pow(*i) <= left_document.buffer.piece_table.num_lines())
                    .count()
                    + 2;

                let left_layout = RenderLayout {
                    row_offset: 0,
                    col_offset: left_numbers_num_cols,
                    num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize
                        - left_numbers_num_cols,
                };

                let left_numbers_layout = RenderLayout {
                    row_offset: 0,
                    col_offset: 0,
                    num_rows: left_layout.num_rows,
                    num_cols: left_numbers_num_cols.saturating_sub(2),
                };

                let left_status_line_layout = RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: 0,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                };

                let right_document = &mut self.open_documents[j];
                let right_numbers_num_cols = (0..)
                    .take_while(|i| {
                        10usize.pow(*i) <= right_document.buffer.piece_table.num_lines()
                    })
                    .count()
                    + 2;

                let right_layout = RenderLayout {
                    row_offset: 0,
                    col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize
                        + right_numbers_num_cols,
                    num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                };

                let right_numbers_layout = RenderLayout {
                    row_offset: 0,
                    col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    num_rows: right_layout.num_rows,
                    num_cols: right_numbers_num_cols.saturating_sub(2),
                };

                let right_status_line_layout = RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                };

                [
                    DocumentLayout {
                        layout: left_layout,
                        numbers_layout: left_numbers_layout,
                        status_line_layout: left_status_line_layout,
                    },
                    DocumentLayout {
                        layout: right_layout,
                        numbers_layout: right_numbers_layout,
                        status_line_layout: right_status_line_layout,
                    },
                ]
            }
            [Some(i), _] => {
                let document = &mut self.open_documents[i];
                let numbers_num_cols = (0..)
                    .take_while(|i| 10usize.pow(*i) <= document.buffer.piece_table.num_lines())
                    .count()
                    + 2;

                let layout = RenderLayout {
                    row_offset: 0,
                    col_offset: numbers_num_cols,
                    num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                    num_cols: if self.split_view {
                        (window_size.0 / font_size.0 / 2.0).ceil() as usize - numbers_num_cols
                    } else {
                        (window_size.0 / font_size.0).ceil() as usize - numbers_num_cols
                    },
                };

                let numbers_layout = RenderLayout {
                    row_offset: 0,
                    col_offset: 0,
                    num_rows: layout.num_rows,
                    num_cols: numbers_num_cols.saturating_sub(2),
                };

                let status_line_layout = RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: 0,
                    num_rows: 2,
                    num_cols: if self.split_view {
                        (window_size.0 / font_size.0 / 2.0).ceil() as usize
                    } else {
                        (window_size.0 / font_size.0).ceil() as usize
                    },
                };

                let right_layout = if self.split_view {
                    DocumentLayout {
                        layout: RenderLayout {
                            row_offset: 0,
                            col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                            num_rows: ((window_size.1 / font_size.1).ceil() as usize)
                                .saturating_sub(1),
                            num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                        },
                        numbers_layout: RenderLayout::default(),
                        status_line_layout: RenderLayout {
                            row_offset: ((window_size.1 / font_size.1).ceil() as usize)
                                .saturating_sub(2),
                            col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                            num_rows: 2,
                            num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                        },
                    }
                } else {
                    DocumentLayout::default()
                };

                [
                    DocumentLayout {
                        layout,
                        numbers_layout,
                        status_line_layout,
                    },
                    right_layout,
                ]
            }
            [None, Some(i)] => {
                if !self.split_view {
                    [DocumentLayout::default(), DocumentLayout::default()]
                } else {
                    let left_layout = DocumentLayout {
                        layout: RenderLayout {
                            row_offset: 0,
                            col_offset: 0,
                            num_rows: ((window_size.1 / font_size.1).ceil() as usize)
                                .saturating_sub(1),
                            num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                        },
                        numbers_layout: RenderLayout::default(),
                        status_line_layout: RenderLayout {
                            row_offset: ((window_size.1 / font_size.1).ceil() as usize)
                                .saturating_sub(2),
                            col_offset: 0,
                            num_rows: 2,
                            num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                        },
                    };

                    let right_document = &mut self.open_documents[i];
                    let right_numbers_num_cols = (0..)
                        .take_while(|i| {
                            10usize.pow(*i) <= right_document.buffer.piece_table.num_lines()
                        })
                        .count()
                        + 2;

                    let right_layout = RenderLayout {
                        row_offset: 0,
                        col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize
                            + right_numbers_num_cols,
                        num_rows: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(1),
                        num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    };

                    let right_numbers_layout = RenderLayout {
                        row_offset: 0,
                        col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                        num_rows: right_layout.num_rows,
                        num_cols: right_numbers_num_cols.saturating_sub(2),
                    };

                    let right_status_line_layout = RenderLayout {
                        row_offset: ((window_size.1 / font_size.1).ceil() as usize)
                            .saturating_sub(2),
                        col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                        num_rows: 2,
                        num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    };

                    [
                        left_layout,
                        DocumentLayout {
                            layout: right_layout,
                            numbers_layout: right_numbers_layout,
                            status_line_layout: right_status_line_layout,
                        },
                    ]
                }
            }
            [None, None] => {
                let left_status_line_layout = RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: 0,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                };

                let right_status_line_layout = RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0 / 2.0).ceil() as usize,
                };

                [
                    DocumentLayout {
                        status_line_layout: left_status_line_layout,
                        ..Default::default()
                    },
                    DocumentLayout {
                        status_line_layout: right_status_line_layout,
                        ..Default::default()
                    },
                ]
            }
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

    pub fn open_workspace(&mut self, window: &Window) -> bool {
        if let Some(path) = platform_resources::open_folder(window) {
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
                                for document in &self.open_documents {
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
                                if let Some(i) = self.visible_documents[self.active_view] {
                                    self.open_documents[i]
                                        .buffer
                                        .update_completions(&mut server);
                                }
                                require_redraw = true;
                            }
                            "textDocument/signatureHelp" => {
                                if let Some(value) = response.value {
                                    server.save_signature_help(response.id, value);
                                }
                                if let Some(i) = self.visible_documents[self.active_view] {
                                    self.open_documents[i]
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

        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );
        let font_size = self.renderer.get_font_size();

        if let Some(left_document) = self.visible_documents.get(0).unwrap() {
            self.renderer
                .clear_layout_contents(&self.visible_documents_layouts[0].layout);
            self.renderer
                .clear_layout_contents(&self.visible_documents_layouts[0].numbers_layout);
            self.renderer.draw_buffer(
                &self.open_documents[*left_document].buffer,
                &self.visible_documents_layouts[0].layout,
                &self.open_documents[*left_document].view,
                &self.open_documents[*left_document].buffer.language_server,
            );

            self.renderer.draw_numbers(
                &self.open_documents[*left_document].buffer,
                &self.visible_documents_layouts[0].numbers_layout,
                &self.open_documents[*left_document].view,
            );

            self.renderer.draw_status_line(
                &self.workspace,
                Some(self.open_documents[*left_document].path.clone()),
                &self.visible_documents_layouts[0].status_line_layout,
            );
        }

        if let Some(right_document) = self.visible_documents.get(1).unwrap() {
            self.renderer
                .clear_layout_contents(&self.visible_documents_layouts[1].layout);
            self.renderer
                .clear_layout_contents(&self.visible_documents_layouts[1].numbers_layout);
            self.renderer.draw_buffer(
                &self.open_documents[*right_document].buffer,
                &self.visible_documents_layouts[1].layout,
                &self.open_documents[*right_document].view,
                &self.open_documents[*right_document].buffer.language_server,
            );

            self.renderer.draw_numbers(
                &self.open_documents[*right_document].buffer,
                &self.visible_documents_layouts[1].numbers_layout,
                &self.open_documents[*right_document].view,
            );

            self.renderer.draw_status_line(
                &self.workspace,
                Some(self.open_documents[*right_document].path.clone()),
                &self.visible_documents_layouts[1].status_line_layout,
            );
        } else if self.split_view {
            self.renderer
                .clear_layout_contents(&self.visible_documents_layouts[1].layout);
        }

        if self.split_view {
            if self.visible_documents[0].is_none() {
                self.renderer.draw_status_line(
                    &self.workspace,
                    None,
                    &self.visible_documents_layouts[0].status_line_layout,
                );
            }
            if self.visible_documents[1].is_none() {
                self.renderer.draw_status_line(
                    &self.workspace,
                    None,
                    &self.visible_documents_layouts[1].status_line_layout,
                );
            }
            self.renderer.draw_split(window);
        } else if self.visible_documents == [None, None] {
            self.renderer.draw_status_line(
                &self.workspace,
                None,
                &RenderLayout {
                    row_offset: ((window_size.1 / font_size.1).ceil() as usize).saturating_sub(2),
                    col_offset: 0,
                    num_rows: 2,
                    num_cols: (window_size.0 / font_size.0).ceil() as usize,
                },
            );
        }

        if let (Some(workspace), Some(file_finder)) = (&self.workspace, &self.file_finder) {
            self.renderer.draw_file_finder(
                &mut self.file_finder_layout,
                &workspace.path,
                file_finder,
            );
        }

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
        window: &Window,
    ) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view] {
            let (line, col) = self.open_documents[i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                self.open_documents[i].buffer.insert_cursor(line, col);
            } else {
                self.open_documents[i].buffer.set_cursor(line, col);
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

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view] {
            let (line, col) = self.open_documents[i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            self.open_documents[i].buffer.set_drag(line, col);
        }
    }

    pub fn handle_mouse_double_click(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        modifiers: Option<ModifiersState>,
        window: &Window,
    ) -> bool {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }
        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view] {
            let (line, col) = self.open_documents[i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            if modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::SHIFT)) {
                self.open_documents[i].buffer.insert_cursor(line, col);
            } else if self.open_documents[i]
                .buffer
                .handle_mouse_double_click(line, col)
            {
                return true;
            }
        }
        false
    }

    pub fn handle_scroll(
        &mut self,
        mouse_position: LogicalPosition<f64>,
        sign: isize,
        window: &Window,
    ) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }

        if let Some(i) = self.visible_documents[self.active_view] {
            let document = &mut self.open_documents[i];
            document.view.handle_scroll(&document.buffer, sign);
        }
    }

    pub fn handle_mouse_hover(&mut self, mouse_position: LogicalPosition<f64>, window: &Window) {
        let window_size = (
            window.inner_size().width as f64 / window.scale_factor(),
            window.inner_size().height as f64 / window.scale_factor(),
        );

        if self.split_view {
            self.active_view = if mouse_position.x < window_size.0 / 2.0 {
                0
            } else {
                1
            }
        }

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view] {
            let document = &mut self.open_documents[i];
            document
                .view
                .hover(&active_document_layout.layout, mouse_position, font_size);
        }
    }

    pub fn handle_mouse_exit_hover(&mut self) {
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view] {
            self.open_documents[i].view.exit_hover();
        }
    }

    pub fn hovering(&mut self) -> bool {
        if let Some(i) = self.visible_documents[self.active_view] {
            return self.open_documents[i].view.hover.is_some();
        }
        false
    }

    pub fn has_moved_cell(
        &mut self,
        cached_mouse_position: LogicalPosition<f64>,
        mouse_position: LogicalPosition<f64>,
    ) -> bool {
        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        if let Some(i) = self.visible_documents[self.active_view] {
            let (line, col) = self.open_documents[i].view.get_line_col(
                &active_document_layout.layout,
                mouse_position,
                font_size,
            );
            return (line, col)
                != self.open_documents[i].view.get_line_col(
                    &active_document_layout.layout,
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
                self.split_view = !self.split_view;
                return true;
            }
            VirtualKeyCode::C if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                self.renderer.cycle_theme();

                for document in &mut self.open_documents {
                    document.buffer.syntect_reload(&self.renderer.theme);
                }

                return true;
            }
            VirtualKeyCode::O if modifiers.is_some_and(|m| m.contains(ModifiersState::CTRL)) => {
                if self.ready_to_quit() && self.open_workspace(window) {
                    self.open_documents.clear();
                    self.active_view = 0;
                    self.visible_documents = [None, None];
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

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();
        let mut delayed_command = None;
        if let Some(i) = self.visible_documents[self.active_view] {
            let document = &mut self.open_documents[i];

            if let Some(editor_command) = document.buffer.handle_key(
                key_code,
                modifiers,
                &document.view,
                &active_document_layout.layout,
            ) {
                match editor_command {
                    EditorCommand::CenterView => document
                        .view
                        .center(&document.buffer, &active_document_layout.layout),
                    EditorCommand::CenterIfNotVisible => document
                        .view
                        .center_if_not_visible(&document.buffer, &active_document_layout.layout),
                    EditorCommand::ToggleSplitView => {
                        self.split_view = !self.split_view;
                    }
                    x => delayed_command = Some(x),
                }
                document
                    .view
                    .adjust(&document.buffer, &active_document_layout.layout)
            }
        }

        if let Some(command) = delayed_command {
            match command {
                EditorCommand::Quit => {
                    let ready_to_quit = self.visible_documents[self.active_view]
                        .is_some_and(|i| self.open_documents[i].buffer.ready_to_quit());

                    if ready_to_quit {
                        self.open_documents
                            .remove(self.visible_documents[self.active_view].unwrap());

                        if self.open_documents.is_empty() {
                            self.visible_documents = [None, None];
                        } else {
                            for index in &mut self.visible_documents {
                                if let Some(index) = index {
                                    *index = min(
                                        index.saturating_sub(1),
                                        self.open_documents.len().saturating_sub(1),
                                    );
                                }
                            }
                        }
                    }

                    return !self.open_documents.is_empty();
                }
                EditorCommand::QuitNoCheck => {
                    self.open_documents
                        .remove(self.visible_documents[self.active_view].unwrap());

                    if self.open_documents.is_empty() {
                        self.visible_documents = [None, None];
                    } else {
                        for index in &mut self.visible_documents {
                            if let Some(index) = index {
                                *index = min(
                                    index.saturating_sub(1),
                                    self.open_documents.len().saturating_sub(1),
                                );
                            }
                        }
                    }

                    return !self.open_documents.is_empty();
                }
                EditorCommand::QuitAll => {
                    let ready_to_quit = self.ready_to_quit();
                    if ready_to_quit {
                        self.open_documents.clear();
                        self.active_view = 0;
                        self.visible_documents = [None, None];
                        return false;
                    }
                }
                EditorCommand::QuitAllNoCheck => {
                    self.open_documents.clear();
                    self.active_view = 0;
                    self.visible_documents = [None, None];
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

        let active_document_layout = &self.visible_documents_layouts[self.active_view];
        let font_size = self.renderer.get_font_size();

        let mut delayed_command = None;
        if let Some(i) = self.visible_documents[self.active_view] {
            let document = &mut self.open_documents[i];

            if let Some(editor_command) = document.buffer.handle_char(c) {
                match editor_command {
                    EditorCommand::CenterView => document
                        .view
                        .center(&document.buffer, &active_document_layout.layout),
                    EditorCommand::CenterIfNotVisible => document
                        .view
                        .center_if_not_visible(&document.buffer, &active_document_layout.layout),
                    EditorCommand::ToggleSplitView => {
                        self.split_view = !self.split_view;
                    }
                    x => delayed_command = Some(x),
                }
            }
            document
                .view
                .adjust(&document.buffer, &active_document_layout.layout)
        }

        if let Some(command) = delayed_command {
            match command {
                EditorCommand::Quit => {
                    let ready_to_quit = self.visible_documents[self.active_view]
                        .is_some_and(|i| self.open_documents[i].buffer.ready_to_quit());

                    if ready_to_quit {
                        self.open_documents
                            .remove(self.visible_documents[self.active_view].unwrap());

                        if self.open_documents.is_empty() {
                            self.visible_documents = [None, None];
                        } else {
                            for index in &mut self.visible_documents {
                                if let Some(index) = index {
                                    *index = min(
                                        index.saturating_sub(1),
                                        self.open_documents.len().saturating_sub(1),
                                    );
                                }
                            }
                        }
                    }

                    return !self.open_documents.is_empty();
                }
                EditorCommand::QuitNoCheck => {
                    self.open_documents
                        .remove(self.visible_documents[self.active_view].unwrap());

                    if self.open_documents.is_empty() {
                        self.visible_documents = [None, None];
                    } else {
                        for index in &mut self.visible_documents {
                            if let Some(index) = index {
                                *index = min(
                                    index.saturating_sub(1),
                                    self.open_documents.len().saturating_sub(1),
                                );
                            }
                        }
                    }

                    return !self.open_documents.is_empty();
                }
                EditorCommand::QuitAll => {
                    let ready_to_quit = self.ready_to_quit();
                    self.open_documents.clear();
                    self.active_view = 0;
                    self.visible_documents = [None, None];
                    return false;
                }
                EditorCommand::QuitAllNoCheck => {
                    self.open_documents.clear();
                    self.active_view = 0;
                    self.visible_documents = [None, None];
                    return false;
                }
                _ => (),
            }
        }

        true
    }

    pub fn ready_to_quit(&mut self) -> bool {
        self.open_documents
            .iter_mut()
            .all(|document| document.buffer.ready_to_quit())
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

        if let Some(i) = self
            .open_documents
            .iter()
            .position(|document| document.path == path)
        {
            self.visible_documents[self.active_view] = Some(i);
        } else {
            self.open_documents.push(Document {
                path: path.to_string(),
                buffer: Buffer::new(window, path, &self.renderer.theme, language_server),
                view: View::new(),
            });
            self.visible_documents[self.active_view] =
                Some(self.open_documents.len().saturating_sub(1));
        }
    }

    fn active_document_layout(&self) -> &DocumentLayout {
        &self.visible_documents_layouts[self.active_view]
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
