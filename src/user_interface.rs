use std::{
    cmp::{max, min},
    collections::HashMap,
    ffi::{CStr, CString},
    path::PathBuf,
    ptr::null,
    time::Duration,
};

use imgui::{
    sys::{
        igDockBuilderAddNode, igDockBuilderDockWindow, igDockBuilderFinish, igDockBuilderGetNode,
        igDockBuilderRemoveNode, igDockBuilderSetNodeSize, igDockBuilderSplitNode,
        igDockNodeGetRootNode, igDockSpaceOverViewport, igFindWindowByName, igFocusWindow,
        igGetCurrentWindow, igGetMainViewport, igGetWindowDockID, igGetWindowDockNode,
        igScrollToBringRectIntoView, igSetNextWindowClass, igSetNextWindowDockID, ImGuiDir_Left,
        ImGuiDir_Right, ImGuiDockNodeFlags_CentralNode, ImGuiDockNodeFlags_DockSpace,
        ImGuiDockNodeFlags_NoCloseButton, ImGuiDockNodeFlags_NoDocking,
        ImGuiDockNodeFlags_NoDockingSplitOther, ImGuiDockNodeFlags_NoSplit,
        ImGuiDockNodeFlags_NoTabBar, ImGuiDockNodeFlags_None,
        ImGuiDockNodeFlags_PassthruCentralNode, ImGuiDockNodeState_HostWindowVisible,
        ImGuiWindowClass, ImRect,
    },
    Condition, ConfigFlags, Context, DrawData, FontAtlasTexture, FontConfig, FontSource, Key,
    TextureId, TreeNodeFlags, Ui,
};
use imgui_winit_support::{
    winit::{event::Event, window::Window},
    WinitPlatform,
};
use url::Url;

use crate::{
    buffer::{Buffer, BufferMode},
    editor::{Editor, FileTreeEntry},
    renderer::Renderer,
    theme::{Theme, THEMES},
};

#[derive(Clone, Copy)]
enum View {
    Left,
    Right,
}

pub struct UserInterface {
    context: Context,
    platform: WinitPlatform,

    open_files: Vec<Url>,
    initial_docks: HashMap<Url, u32>,

    first_frame: bool,
    file_tree_view: u32,
    central_view: u32,
    active_view: u32,
}

pub struct RenderData<'a> {
    pub draw_data: &'a DrawData,
    pub buffers: Vec<Url>,
    pub scroll_state: HashMap<Url, (f32, f32)>,
    pub clip_rects: HashMap<Url, ImRect>,
}

impl UserInterface {
    pub fn new(window: &Window) -> Self {
        let mut context = Context::create();
        context.set_ini_filename(None);
        context.io_mut().config_flags |= ConfigFlags::DOCKING_ENABLE;
        context.style_mut().scale_all_sizes(1.5);

        context.fonts().add_font(&[FontSource::TtfData {
            data: include_bytes!("C:/Windows/Fonts/segoeuisl.ttf"),
            size_pixels: 36.0,
            config: Some(FontConfig {
                oversample_h: 4,
                oversample_v: 4,
                ..Default::default()
            }),
        }]);

        let mut platform = WinitPlatform::init(&mut context);
        platform.attach_window(
            context.io_mut(),
            window,
            imgui_winit_support::HiDpiMode::Locked(1.0),
        );

        Self {
            context,
            platform,
            open_files: Vec::new(),
            initial_docks: HashMap::new(),
            first_frame: true,
            file_tree_view: 0,
            central_view: 0,
            active_view: 0,
        }
    }

    pub fn font_atlas_texture(&mut self) -> FontAtlasTexture {
        self.context.fonts().build_rgba32_texture()
    }

    pub fn pre_frame(&mut self, delta: Duration) {
        self.context.io_mut().update_delta_time(delta);
    }

    pub fn prepare_frame(&mut self, window: &Window) {
        self.platform
            .prepare_frame(self.context.io_mut(), window)
            .unwrap();
    }

    pub fn handle_event(&mut self, window: &Window, event: &Event<()>) {
        self.platform
            .handle_event(self.context.io_mut(), window, event);
    }

    pub fn run(
        &mut self,
        window: &Window,
        renderer: &Renderer,
        editor: &mut Editor,
        theme: &mut Theme,
    ) -> Option<RenderData> {
        self.context.fonts().tex_id = TextureId::from(usize::MAX);
        let ui = self.context.new_frame();

        if ui.is_key_down(Key::LeftCtrl) && ui.is_key_pressed(Key::C) {
            cycle_theme(theme);
            for buffer in editor.buffers.values_mut() {
                buffer.syntect_reload(theme);
            }
        }
        if ui.is_key_down(Key::LeftCtrl)
            && ui.is_key_down(Key::LeftShift)
            && ui.is_key_pressed(Key::O)
        {
            editor.open_workspace(window);
        }
        if ui.is_key_down(Key::LeftCtrl)
            && !ui.is_key_down(Key::LeftShift)
            && ui.is_key_pressed(Key::O)
        {
            if let Some(file) = editor.open_file_prompt(window, theme) {
                let window_name = CString::new(Into::<String>::into(file.clone())).unwrap();
                let window = unsafe { igFindWindowByName(window_name.as_ptr()) };
                if !window.is_null() && unsafe { (*window).Appearing } {
                    unsafe {
                        igFocusWindow(window);
                    }
                } else {
                    self.open_files.push(file.clone());
                    self.initial_docks.insert(file.clone(), self.active_view);
                }
            }
        }

        unsafe {
            let dockspace_id = igDockSpaceOverViewport(
                igGetMainViewport(),
                ImGuiDockNodeFlags_PassthruCentralNode as i32,
                null(),
            );

            if self.first_frame {
                igDockBuilderRemoveNode(dockspace_id);
                igDockBuilderAddNode(dockspace_id, ImGuiDockNodeFlags_None as i32);
                igDockBuilderSetNodeSize(dockspace_id, (*igGetMainViewport()).Size);

                igDockBuilderSplitNode(
                    dockspace_id,
                    ImGuiDir_Left,
                    0.1,
                    &mut self.file_tree_view,
                    &mut self.central_view,
                );

                (*igDockBuilderGetNode(self.central_view)).LocalFlags =
                    ImGuiDockNodeFlags_CentralNode;
                (*igDockBuilderGetNode(self.file_tree_view)).LocalFlags =
                    ImGuiDockNodeFlags_NoTabBar | ImGuiDockNodeFlags_NoDocking;
                igDockBuilderDockWindow(b"File Tree\0".as_ptr().cast(), self.file_tree_view);

                igDockBuilderFinish(dockspace_id);

                self.active_view = self.central_view;
                self.first_frame = false;
            }
        }

        if let Some(menu) = ui.begin_main_menu_bar() {
            if let Some(file_menu) = ui.begin_menu("File") {
                file_menu.end();
            }
            menu.end();
        }

        ui.window("File Tree").horizontal_scrollbar(true).build(|| {
            let mut file_to_open: Option<PathBuf> = None;
            if let Some(workspace) = &editor.workspace {
                fn show_entry(ui: &Ui, entry: &FileTreeEntry, file_to_open: &mut Option<PathBuf>) {
                    match entry {
                        FileTreeEntry::File(path) => {
                            if ui.selectable(path.file_name().unwrap().to_str().unwrap()) {
                                *file_to_open = Some(path.clone());
                            }
                        }
                        FileTreeEntry::Folder(path, entries) => {
                            ui.tree_node_config(path.file_name().unwrap().to_str().unwrap())
                                .flags(TreeNodeFlags::SPAN_FULL_WIDTH)
                                .build(|| {
                                    for entry in entries {
                                        show_entry(ui, entry, file_to_open);
                                    }
                                });
                        }
                    }
                }

                ui.tree_node_config(
                    &workspace
                        .uri
                        .to_file_path()
                        .unwrap()
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap(),
                )
                .opened(true, Condition::FirstUseEver)
                .flags(TreeNodeFlags::SPAN_FULL_WIDTH)
                .build(|| {
                    for entry in &workspace.file_tree {
                        show_entry(ui, entry, &mut file_to_open);
                    }
                });
            }

            if let Some(file) = file_to_open {
                if let Some(file) = editor.open_file(window, theme, file.to_str().unwrap()) {let window_name = CString::new(Into::<String>::into(file.clone())).unwrap();
                    let window_name = CString::new(Into::<String>::into(file.clone())).unwrap();
                    let window = unsafe { igFindWindowByName(window_name.as_ptr()) };
                    if !window.is_null() && unsafe { !(*window).DockNode.is_null() } {
                        unsafe {
                            igFocusWindow(window);
                        }
                    } else {
                        self.open_files.push(file.clone());
                        self.initial_docks.insert(file.clone(), self.active_view);
                    }
                }
            }
        });

        let mut buffers = Vec::new();
        let mut scroll_state = HashMap::new();
        let mut clip_rects = HashMap::new();
        let mut file_to_remove = None;
        for file in &self.open_files {
            unsafe {
                igSetNextWindowClass(&ImGuiWindowClass {
                    DockNodeFlagsOverrideSet: ImGuiDockNodeFlags_NoCloseButton as i32,
                    ..Default::default()
                });
                if let Some(dock_id) = self.initial_docks.remove(file) {
                    let dock_node = igDockBuilderGetNode(dock_id);
                    igSetNextWindowDockID(
                        if !dock_node.is_null()
                            && (*dock_node).State == ImGuiDockNodeState_HostWindowVisible
                        {
                            (*dock_node).ID
                        } else {
                            self.central_view
                        },
                        Condition::Always as i32,
                    );
                }
            }

            let document_width =
                editor.buffers[file].piece_table.longest_line() as f32 * renderer.font_size.0;
            let document_height =
                (editor.buffers[file].piece_table.num_lines()) as f32 * renderer.font_size.1;

            let mut remain_open = true;

            ui.window(Into::<String>::into(file.clone()).as_str())
                .opened(&mut remain_open)
                .content_size([document_width, document_height])
                .horizontal_scrollbar(true)
                .build(|| {
                    add_selections(ui, theme, renderer.font_size, &editor.buffers[file]);
                    add_cursor_leads(ui, theme, renderer.font_size, &editor.buffers[file]);

                    ui.get_window_draw_list()
                        .add_image(TextureId::new(buffers.len()), [0.0, 0.0], [0.0, 0.0])
                        .build();
                    buffers.push(file.clone());
                    scroll_state.insert(file.clone(), (ui.scroll_x(), ui.scroll_y()));
                    clip_rects.insert(file.clone(), unsafe {
                        (*igGetCurrentWindow()).InnerClipRect
                    });

                    if ui.is_window_focused() {
                        let dock_node = unsafe { igGetWindowDockNode() };
                        if !dock_node.is_null() {
                            self.active_view = unsafe { *dock_node }.ID;
                        }
                        handle_buffer_input(
                            ui,
                            renderer.font_size,
                            editor.buffers.get_mut(file).unwrap(),
                        );
                    }
                });

            if !remain_open {
                buffers.pop();
                editor.close_file(file);
                file_to_remove = Some(file.clone());
            }
        }

        if let Some(file) = &file_to_remove {
            self.open_files.retain(|f| f != file);
        }

        self.platform.prepare_render(ui, window);
        Some(RenderData {
            draw_data: self.context.render(),
            buffers,
            scroll_state,
            clip_rects,
        })
    }
}

fn cycle_theme(theme: &mut Theme) {
    let i = THEMES.iter().position(|t| *t == *theme).unwrap();
    *theme = THEMES[(i + 1) % THEMES.len()];
}

fn handle_buffer_input(ui: &Ui, font_size: (f32, f32), buffer: &mut Buffer) {
    let mut adjust_view = false;
    for c in ui.io().input_queue_characters().filter(|c| c.is_ascii()) {
        buffer.handle_char(c);
        adjust_view = true;
    }

    for key in [
        Key::DownArrow,
        Key::UpArrow,
        Key::RightArrow,
        Key::LeftArrow,
        Key::Escape,
        Key::Backspace,
        Key::Enter,
        Key::Delete,
        Key::Slash,
        Key::Tab,
        Key::Space,
        Key::R,
        Key::J,
        Key::K,
    ] {
        if ui.is_key_pressed(key) {
            buffer.handle_key(key, ui.is_key_down(Key::LeftCtrl));
            adjust_view = true;
        }
    }

    if adjust_view {
        if let Some(last_cursor) = buffer.cursors.last() {
            let (line, col) = last_cursor.get_line_col(&buffer.piece_table);
            let rect = line_col_to_rect(ui, line, col, (1, 1), font_size);
            unsafe {
                igScrollToBringRectIntoView(igGetCurrentWindow(), rect);
            }
        }
    }
}

fn line_col_to_rect(
    ui: &Ui,
    line: usize,
    col: usize,
    size: (usize, usize),
    font_size: (f32, f32),
) -> ImRect {
    let clip_rect = unsafe { (*igGetCurrentWindow()).InnerClipRect };
    let scroll_state = [ui.scroll_x(), ui.scroll_y()];

    let min = [
        clip_rect.Min.x + col as f32 * font_size.0 - scroll_state[0],
        clip_rect.Min.y + line as f32 * font_size.1 - scroll_state[1],
    ];
    let max = [
        clip_rect.Min.x + (col + size.0) as f32 * font_size.0 - scroll_state[0],
        clip_rect.Min.y + (line + size.1) as f32 * font_size.1 - scroll_state[1],
    ];
    ImRect {
        Min: min.into(),
        Max: max.into(),
    }
}

fn add_cursor_leads(ui: &Ui, theme: &Theme, font_size: (f32, f32), buffer: &Buffer) {
    for cursor in &buffer.cursors {
        let (line, col) = cursor.get_line_col(&buffer.piece_table);
        let mut rect = line_col_to_rect(ui, line, col, (1, 1), font_size);
        if buffer.mode == BufferMode::Insert {
            rect.Max.x -= 0.85 * font_size.0;
        }

        ui.get_window_draw_list()
            .add_rect(
                [rect.Min.x, rect.Min.y],
                [rect.Max.x, rect.Max.y],
                theme.cursor_color.into_imgui(),
            )
            .filled(true)
            .build();
    }
}

fn add_selections(ui: &Ui, theme: &Theme, font_size: (f32, f32), buffer: &Buffer) {
    if buffer.mode == BufferMode::VisualLine {
        for cursor in buffer.cursors.iter() {
            let line = buffer.piece_table.line_index(cursor.position);
            let anchor_line = buffer.piece_table.line_index(cursor.anchor);
            for line in min(line, anchor_line)..=max(line, anchor_line) {
                let start = 0;
                let end = buffer.piece_table.line_at_index(line).unwrap().length;
                let rect = line_col_to_rect(ui, line, start, (end - start + 1, 1), font_size);
                ui.get_window_draw_list()
                    .add_rect(
                        [rect.Min.x, rect.Min.y],
                        [rect.Max.x, rect.Max.y],
                        theme.selection_background_color.into_imgui(),
                    )
                    .filled(true)
                    .build();
            }
        }
    } else if buffer.mode == BufferMode::Visual {
        for cursor in buffer.cursors.iter() {
            for range in cursor.get_selection_ranges(&buffer.piece_table) {
                let rect = line_col_to_rect(
                    ui,
                    range.line,
                    range.start,
                    (range.end - range.start + 1, 1),
                    font_size,
                );
                ui.get_window_draw_list()
                    .add_rect(
                        [rect.Min.x, rect.Min.y],
                        [rect.Max.x, rect.Max.y],
                        theme.selection_background_color.into_imgui(),
                    )
                    .filled(true)
                    .build();
            }
        }
    }
}
