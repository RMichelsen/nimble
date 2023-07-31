use std::{
    cmp::{max, min},
    collections::HashMap,
    ffi::CString,
    path::PathBuf,
    ptr::null,
    time::{Duration, Instant},
};

use imgui::{
    sys::{
        igDockBuilderAddNode, igDockBuilderDockWindow, igDockBuilderFinish, igDockBuilderGetNode,
        igDockBuilderRemoveNode, igDockBuilderSetNodeSize, igDockBuilderSplitNode,
        igDockSpaceOverViewport, igFindWindowByName, igFocusWindow, igGetCurrentWindow,
        igGetMainViewport, igGetStyle, igGetWindowDockNode, igScrollToBringRectIntoView,
        igScrollToItem, igSetNextWindowClass, igSetNextWindowDockID, igStyleColorsDark,
        igStyleColorsLight, ImGuiCol_Border, ImGuiCol_Button, ImGuiCol_ButtonActive,
        ImGuiCol_ButtonHovered, ImGuiCol_CheckMark, ImGuiCol_DockingPreview, ImGuiCol_FrameBg,
        ImGuiCol_FrameBgActive, ImGuiCol_FrameBgHovered, ImGuiCol_Header, ImGuiCol_HeaderActive,
        ImGuiCol_HeaderHovered, ImGuiCol_MenuBarBg, ImGuiCol_ModalWindowDimBg,
        ImGuiCol_NavHighlight, ImGuiCol_PopupBg, ImGuiCol_ResizeGrip, ImGuiCol_ResizeGripActive,
        ImGuiCol_ResizeGripHovered, ImGuiCol_ScrollbarBg, ImGuiCol_ScrollbarGrab,
        ImGuiCol_ScrollbarGrabActive, ImGuiCol_ScrollbarGrabHovered, ImGuiCol_Separator,
        ImGuiCol_SeparatorActive, ImGuiCol_SeparatorHovered, ImGuiCol_SliderGrab,
        ImGuiCol_SliderGrabActive, ImGuiCol_Tab, ImGuiCol_TabActive, ImGuiCol_TabHovered,
        ImGuiCol_TabUnfocused, ImGuiCol_TabUnfocusedActive, ImGuiCol_Text, ImGuiCol_TextDisabled,
        ImGuiCol_TextSelectedBg, ImGuiCol_TitleBg, ImGuiCol_TitleBgActive, ImGuiCol_WindowBg,
        ImGuiDir_Left, ImGuiDockNodeFlags_CentralNode, ImGuiDockNodeFlags_NoCloseButton,
        ImGuiDockNodeFlags_NoDocking, ImGuiDockNodeFlags_NoTabBar, ImGuiDockNodeFlags_None,
        ImGuiDockNodeFlags_PassthruCentralNode, ImGuiDockNodeState_HostWindowVisible,
        ImGuiScrollFlags_None, ImGuiWindowClass, ImRect,
    },
    Condition, ConfigFlags, Context, DrawData, FontAtlasTexture, FontConfig, FontId, FontSource,
    Key, MouseButton, TextureId, TreeNodeFlags, Ui, 
};
use imgui_winit_support::{
    winit::{event::Event, window::Window},
    WinitPlatform,
};
use url::Url;

use crate::{
    buffer::{Buffer, BufferMode},
    cursor::get_filtered_completions,
    editor::{Editor, FileTreeEntry},
    language_server_types::ParameterLabelType,
    renderer::Renderer,
    text_utils::{self, CharType},
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
    hover_active_last_frame: HashMap<Url, bool>,
    hovers: HashMap<Url, (Instant, usize, usize)>,

    first_frame: bool,
    file_tree_view: u32,
    central_view: u32,
    active_view: u32,

    monospace_font: FontId,
    regular_font: FontId,
}

pub struct RenderData<'a> {
    pub draw_data: &'a DrawData,
    pub buffers: Vec<Url>,
    pub scroll_state: HashMap<Url, (f32, f32)>,
    pub clip_rects: HashMap<Url, ImRect>,
}

impl UserInterface {
    pub fn new(window: &Window, theme: &Theme) -> Self {
        let mut context = Context::create();
        context.set_ini_filename(None);
        context.io_mut().config_flags |= ConfigFlags::DOCKING_ENABLE;
        context.style_mut().scale_all_sizes(1.5);

        let monospace_font = context.fonts().add_font(&[FontSource::TtfData {
            data: include_bytes!("C:/Windows/Fonts/consola.ttf"),
            size_pixels: 26.0,
            config: Some(FontConfig {
                oversample_h: 4,
                oversample_v: 4,
                ..Default::default()
            }),
        }]);
        let regular_font = context.fonts().add_font(&[FontSource::TtfData {
            data: include_bytes!("../resources/FiraSans-Regular.ttf"),
            size_pixels: 30.0,
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
        context.io_mut().display_size = window.inner_size().into();

        set_theme(theme);

        Self {
            context,
            platform,
            open_files: Vec::new(),
            initial_docks: HashMap::new(),
            hover_active_last_frame: HashMap::new(),
            hovers: HashMap::new(),
            first_frame: true,
            file_tree_view: 0,
            central_view: 0,
            active_view: 0,
            monospace_font,
            regular_font,
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

    pub fn resize(&mut self, window: &Window) {
        self.platform.attach_window(
            self.context.io_mut(),
            window,
            imgui_winit_support::HiDpiMode::Locked(1.0),
        );
        self.context.io_mut().display_size = window.inner_size().into();
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

        let font = ui.push_font(self.regular_font);

        if ui.is_key_down(Key::LeftCtrl) && ui.is_key_pressed(Key::C) {
            cycle_theme(theme);
            for buffer in editor.buffers.values_mut() {
                buffer.syntect_reload(theme);
            }
            set_theme(theme);
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
                let window_name = CString::new(
                    file.to_file_path()
                        .unwrap()
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string()
                        + "##"
                        + Into::<String>::into(file.clone()).as_str(),
                )
                .unwrap();
                let window = unsafe { igFindWindowByName(window_name.as_ptr()) };
                if !window.is_null() && unsafe { (*window).Appearing } {
                    unsafe {
                        igFocusWindow(window);
                    }
                } else {
                    self.open_files.push(file.clone());
                    self.initial_docks.insert(file, self.active_view);
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
                if let Some(file) = editor.open_file(window, theme, file.to_str().unwrap()) {
                    let window_name = CString::new(Into::<String>::into(file.clone())).unwrap();
                    let window_name = CString::new(
                        file.to_file_path()
                            .unwrap()
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_string()
                            + "##"
                            + Into::<String>::into(file.clone()).as_str(),
                    )
                    .unwrap();
                    let window = unsafe { igFindWindowByName(window_name.as_ptr()) };
                    if !window.is_null() && unsafe { !(*window).DockNode.is_null() } {
                        unsafe {
                            igFocusWindow(window);
                        }
                    } else {
                        self.open_files.push(file.clone());
                        self.initial_docks.insert(file, self.active_view);
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
                    DockNodeFlagsOverrideSet: ImGuiDockNodeFlags_NoCloseButton,
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

            let window_name = file
                .to_file_path()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
                + "##"
                + Into::<String>::into(file.clone()).as_str();
            ui.window(&window_name)
                .opened(&mut remain_open)
                .content_size([document_width, document_height])
                .horizontal_scrollbar(true)
                .build(|| {
                    add_selections(ui, theme, renderer.font_size, &editor.buffers[file]);
                    add_cursor_leads(ui, theme, renderer.font_size, &editor.buffers[file]);

                    ui.get_window_draw_list()
                        .add_image(TextureId::new(buffers.len()), [0.0, 0.0], [0.0, 0.0])
                        .build();

                    add_diagnostics(ui, theme, renderer.font_size, &editor.buffers[file]);

                    let font = ui.push_font(self.monospace_font);
                    add_signature_helps(ui, theme, renderer.font_size, &editor.buffers[file]);
                    add_completions(
                        ui,
                        theme,
                        renderer.font_size,
                        editor.buffers.get_mut(file).unwrap(),
                    );
                    font.pop();

                    buffers.push(file.clone());
                    scroll_state.insert(file.clone(), (ui.scroll_x(), ui.scroll_y()));
                    clip_rects.insert(file.clone(), unsafe {
                        (*igGetCurrentWindow()).InnerClipRect
                    });

                    if ui.is_window_hovered() {
                        let mouse_pos = ui.io().mouse_pos;
                        let window_pos = ui.window_pos();
                        let relative_mouse_pos = (
                            mouse_pos[0] - window_pos[0] - unsafe { ui.style() }.frame_padding[0] + ui.scroll_x(),
                            mouse_pos[1] - window_pos[1] - (unsafe { ui.style() }.frame_padding[1] * 2.0 + ui.current_font_size()) + ui.scroll_y(),
                        );
                        if relative_mouse_pos.0 > 0.0 && relative_mouse_pos.1 > 0.0 {
                            let line = (relative_mouse_pos.1 / renderer.font_size.1) as usize;
                            let col = (relative_mouse_pos.0 / renderer.font_size.0) as usize;

                            if ui.is_window_focused() && ui.is_mouse_double_clicked(MouseButton::Left) {
                                editor
                                    .buffers
                                    .get_mut(file)
                                    .unwrap()
                                    .handle_double_click(line, col);
                            } else if ui.is_window_focused() && ui.is_mouse_dragging(MouseButton::Left) {
                                editor.buffers.get_mut(file).unwrap().handle_drag(line, col);
                            } else if ui.is_window_focused() && ui.is_mouse_clicked(MouseButton::Left) {
                                editor
                                    .buffers
                                    .get_mut(file)
                                    .unwrap()
                                    .handle_click(line, col);
                            } else if !self.hover_active_last_frame.get(&file).is_some_and(|b| *b) {
                                if let Some(hover) = self.hovers.get_mut(file) {
                                    if hover.1 != line || hover.2 != col {
                                        hover.0 = Instant::now();
                                        hover.1 = line;
                                        hover.2 = col;
                                        editor
                                            .buffers
                                            .get_mut(file)
                                            .unwrap()
                                            .handle_hover(line, col);
                                    }
                                } else {
                                    self.hovers
                                        .insert(file.clone(), (Instant::now(), line, col));
                                        editor
                                            .buffers
                                            .get_mut(file)
                                            .unwrap()
                                            .handle_hover(line, col);
                                }
                            }
                        }
                    }

                    self.hover_active_last_frame.insert(file.clone(), if self
                        .hovers
                        .get(file)
                        .is_some_and(|hover| hover.0.elapsed() > Duration::from_millis(200))
                    {
                        add_hovers(ui, theme, renderer.font_size, &editor.buffers[file])
                    } else {
                        false
                    });

                    if ui.is_window_focused() {
                        let dock_node = unsafe { igGetWindowDockNode() };
                        if !dock_node.is_null() {
                            self.active_view = unsafe { *dock_node }.ID;
                        }
                        if handle_buffer_input(
                            ui,
                            renderer.font_size,
                            editor.buffers.get_mut(file).unwrap(),
                        ) {
                            let buffer = editor.buffers.get(file).unwrap();
                            if let Some(last_cursor) = buffer.cursors.last() {
                                let (line, col) = last_cursor.get_line_col(&buffer.piece_table);
                                let rect =
                                    line_col_to_rect(ui, line, col, (1, 1), renderer.font_size);
                                unsafe {
                                    igScrollToBringRectIntoView(igGetCurrentWindow(), rect);
                                }
                            }
                        }
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

        font.pop();

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

fn set_theme(theme: &Theme) {
    let average_background =
        (theme.background_color.r + theme.background_color.g + theme.background_color.b) / 3.0;
    unsafe {
        let style = igGetStyle();
        if average_background < 0.5 {
            igStyleColorsDark(style);
        } else {
            igStyleColorsLight(style);
        }
        (*style).Colors[ImGuiCol_WindowBg as usize] = theme.background_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_PopupBg as usize] = theme.background_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_MenuBarBg as usize] = theme.background_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_ScrollbarBg as usize] = theme.background_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_TitleBg as usize] = theme.background_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_ModalWindowDimBg as usize] =
            theme.background_color.into_imvec(0.0);
        (*style).Colors[ImGuiCol_TitleBgActive as usize] = theme.background_color.into_imvec(1.2);
        (*style).Colors[ImGuiCol_Text as usize] = theme.foreground_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_Border as usize] = theme.foreground_color.into_imvec(1.0);
        (*style).Colors[ImGuiCol_TextDisabled as usize] = theme.foreground_color.into_imvec(0.5);
        (*style).Colors[ImGuiCol_ScrollbarGrab as usize] = theme.foreground_color.into_imvec(0.6);
        (*style).Colors[ImGuiCol_ScrollbarGrabHovered as usize] =
            theme.foreground_color.into_imvec(0.7);
        (*style).Colors[ImGuiCol_ScrollbarGrabActive as usize] =
            theme.foreground_color.into_imvec(0.95);
        (*style).Colors[ImGuiCol_Separator as usize] = theme.foreground_color.into_imvec(0.3);
        (*style).Colors[ImGuiCol_SeparatorHovered as usize] =
            theme.foreground_color.into_imvec(0.5);
        (*style).Colors[ImGuiCol_SeparatorActive as usize] = theme.foreground_color.into_imvec(0.7);
        (*style).Colors[ImGuiCol_FrameBg as usize] = theme.palette.aqua.into_imvec(0.2);
        (*style).Colors[ImGuiCol_FrameBgHovered as usize] = theme.palette.aqua.into_imvec(0.3);
        (*style).Colors[ImGuiCol_FrameBgActive as usize] = theme.palette.aqua.into_imvec(0.4);
        (*style).Colors[ImGuiCol_CheckMark as usize] = theme.palette.aqua.into_imvec(1.0);
        (*style).Colors[ImGuiCol_SliderGrab as usize] = theme.palette.aqua.into_imvec(0.8);
        (*style).Colors[ImGuiCol_SliderGrabActive as usize] = theme.palette.aqua.into_imvec(1.0);
        (*style).Colors[ImGuiCol_Button as usize] = theme.palette.aqua.into_imvec(0.2);
        (*style).Colors[ImGuiCol_ButtonHovered as usize] = theme.palette.aqua.into_imvec(0.3);
        (*style).Colors[ImGuiCol_ButtonActive as usize] = theme.palette.aqua.into_imvec(0.4);
        (*style).Colors[ImGuiCol_Header as usize] = theme.palette.aqua.into_imvec(0.31);
        (*style).Colors[ImGuiCol_HeaderHovered as usize] = theme.palette.aqua.into_imvec(0.4);
        (*style).Colors[ImGuiCol_HeaderActive as usize] = theme.palette.aqua.into_imvec(0.5);
        (*style).Colors[ImGuiCol_ResizeGrip as usize] = theme.palette.aqua.into_imvec(0.2);
        (*style).Colors[ImGuiCol_ResizeGripHovered as usize] = theme.palette.aqua.into_imvec(0.67);
        (*style).Colors[ImGuiCol_ResizeGripActive as usize] = theme.palette.aqua.into_imvec(0.95);
        (*style).Colors[ImGuiCol_Tab as usize] = theme.palette.aqua.into_imvec(0.2);
        (*style).Colors[ImGuiCol_TabHovered as usize] = theme.palette.aqua.into_imvec(0.3);
        (*style).Colors[ImGuiCol_TabActive as usize] = theme.palette.aqua.into_imvec(0.4);
        (*style).Colors[ImGuiCol_TabUnfocused as usize] = theme.palette.aqua.into_imvec(0.1);
        (*style).Colors[ImGuiCol_TabUnfocusedActive as usize] = theme.palette.aqua.into_imvec(0.2);
        (*style).Colors[ImGuiCol_DockingPreview as usize] = theme.palette.aqua.into_imvec(0.7);
        (*style).Colors[ImGuiCol_NavHighlight as usize] = theme.palette.aqua.into_imvec(1.0);
        (*style).Colors[ImGuiCol_TextSelectedBg as usize] = theme.palette.aqua.into_imvec(0.35);
    }
}

fn handle_buffer_input(ui: &Ui, font_size: (f32, f32), buffer: &mut Buffer) -> bool {
    let mut key_handled = false;
    for c in ui.io().input_queue_characters().filter(|c| c.is_ascii()) {
        buffer.handle_char(c);
        key_handled = true;
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
            key_handled = true;
        }
    }

    key_handled
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
                        theme.selection_background_color.into_imcol(),
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
                        theme.selection_background_color.into_imcol(),
                    )
                    .filled(true)
                    .build();
            }
        }
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
                theme.cursor_color.into_imcol(),
            )
            .filled(true)
            .build();
    }
}

fn add_diagnostics(ui: &Ui, theme: &Theme, font_size: (f32, f32), buffer: &Buffer) {
    if let Some(server) = &buffer.language_server {
        if let Some(diagnostics) = server
            .borrow()
            .saved_diagnostics
            .get(&buffer.uri.to_lowercase())
        {
            for diagnostic in diagnostics {
                let (start_line, start_col) = (
                    diagnostic.range.start.line as usize,
                    diagnostic.range.start.character as usize,
                );
                let (end_line, end_col) = (
                    diagnostic.range.end.line as usize,
                    diagnostic.range.end.character as usize,
                );

                let diagnostic_on_cursor_line = buffer.mode == BufferMode::Insert
                    && buffer.cursors.iter().any(|cursor| {
                        (start_line..=end_line)
                            .contains(&buffer.piece_table.line_index(cursor.position))
                    });

                if diagnostic.severity.is_some_and(|s| s > 2) || diagnostic_on_cursor_line {
                    continue;
                }

                if start_line == end_line {
                    let mut rect = line_col_to_rect(
                        ui,
                        start_line,
                        start_col,
                        (end_col.saturating_sub(start_col) + 1, 1),
                        font_size,
                    );
                    if ui.is_mouse_hovering_rect([rect.Min.x, rect.Min.y], [rect.Max.x, rect.Max.y])
                    {
                        ui.tooltip_text(&diagnostic.message);
                    }
                    rect.Min.y += 0.85 * font_size.1;
                    ui.get_window_draw_list()
                        .add_rect(
                            [rect.Min.x, rect.Min.y],
                            [rect.Max.x, rect.Max.y],
                            theme.diagnostic_color.into_imcol(),
                        )
                        .filled(true)
                        .build();
                } else {
                    let mut rect = line_col_to_rect(
                        ui,
                        start_line,
                        start_col,
                        (
                            buffer.piece_table.line_at_index(start_line).unwrap().length
                                - start_col
                                + 1,
                            1,
                        ),
                        font_size,
                    );
                    if ui.is_mouse_hovering_rect([rect.Min.x, rect.Min.y], [rect.Max.x, rect.Max.y])
                    {
                        ui.tooltip_text(&diagnostic.message);
                    }
                    rect.Min.y += 0.85 * font_size.1;
                    ui.get_window_draw_list()
                        .add_rect(
                            [rect.Min.x, rect.Min.y],
                            [rect.Max.x, rect.Max.y],
                            theme.diagnostic_color.into_imcol(),
                        )
                        .rounding(1.0)
                        .filled(true)
                        .build();

                    for line in start_line + 1..end_line {
                        let mut rect = line_col_to_rect(
                            ui,
                            line,
                            0,
                            (
                                buffer.piece_table.line_at_index(line).unwrap().length + 1,
                                1,
                            ),
                            font_size,
                        );
                        if ui.is_mouse_hovering_rect(
                            [rect.Min.x, rect.Min.y],
                            [rect.Max.x, rect.Max.y],
                        ) {
                            ui.tooltip_text(&diagnostic.message);
                        }
                        rect.Min.y += 0.85 * font_size.1;
                        ui.get_window_draw_list()
                            .add_rect(
                                [rect.Min.x, rect.Min.y],
                                [rect.Max.x, rect.Max.y],
                                theme.diagnostic_color.into_imcol(),
                            )
                            .rounding(1.0)
                            .filled(true)
                            .build();
                    }

                    let mut rect = line_col_to_rect(ui, end_line, 0, (end_col + 1, 1), font_size);
                    if ui.is_mouse_hovering_rect([rect.Min.x, rect.Min.y], [rect.Max.x, rect.Max.y])
                    {
                        ui.tooltip_text(&diagnostic.message);
                    }
                    rect.Min.y += 0.85 * font_size.1;
                    ui.get_window_draw_list()
                        .add_rect(
                            [rect.Min.x, rect.Min.y],
                            [rect.Max.x, rect.Max.y],
                            theme.diagnostic_color.into_imcol(),
                        )
                        .rounding(1.0)
                        .filled(true)
                        .build();
                }
            }
        }
    }
}

fn add_hovers(ui: &Ui, theme: &Theme, font_size: (f32, f32), buffer: &Buffer) -> bool {
    let mut hovering_hover_message = false;
    if let Some(server) = &buffer.language_server {
        if let (line, col, Some(request)) = &buffer.hover_request {
            if let Some(hover) = server.borrow().saved_hover_messages.get(request) {
                let rect = line_col_to_rect(ui, *line + 1, *col, (1, 1), font_size);
                let min_width = ui.window_size()[0] / 2.0;
                let longest_string = hover
                    .contents
                    .value
                    .lines()
                    .max_by(|x, y| x.len().cmp(&y.len()));
                let max_text_width = ui.calc_text_size(longest_string.unwrap_or(""));
                ui.window(format!("Hover##{}", request))
                    .position([rect.Min.x, rect.Min.y], Condition::Always)
                    .size_constraints(
                        [min_width.min(max_text_width[0]), 0.0],
                        [min_width, ui.window_size()[1] / 2.0],
                    )
                    .title_bar(false)
                    .movable(false)
                    .focused(false)
                    .focus_on_appearing(false)
                    .always_auto_resize(true)
                    .build(|| {
                        ui.text_wrapped(&hover.contents.value);
                        hovering_hover_message = ui.is_window_hovered();
                    });
            }
        }
    }
    hovering_hover_message
}

fn add_signature_helps(ui: &Ui, theme: &Theme, font_size: (f32, f32), buffer: &Buffer) {
    if let Some(server) = &buffer.language_server {
        for cursor in buffer.cursors.iter() {
            if let Some(request) = cursor.signature_help_request {
                if let Some(signature_help) = server.borrow().saved_signature_helps.get(&request.id)
                {
                    if signature_help.signatures.is_empty() {
                        return;
                    }
                    let (line, col) = (
                        buffer.piece_table.line_index(request.position),
                        buffer.piece_table.col_index(request.position),
                    );
                    let rect = line_col_to_rect(ui, line.saturating_sub(1), col, (1, 1), font_size);

                    let label_size = ui.calc_text_size(&signature_help.signatures[0].label);
                    ui.window("Signature Help")
                        .position(
                            [
                                rect.Min.x,
                                rect.Min.y
                                    - label_size[1]
                                    - unsafe { ui.style().frame_padding[1] * 2.0 },
                            ],
                            Condition::Always,
                        )
                        .no_inputs()
                        .no_decoration()
                        .movable(false)
                        .focused(false)
                        .focus_on_appearing(false)
                        .always_auto_resize(true)
                        .build(|| {
                            let active_parameter = signature_help.signatures[0]
                                .active_parameter
                                .or(signature_help.active_parameter);
                            if let Some(parameters) = &signature_help.signatures[0].parameters {
                                let mut active_parameter_range = (0, 0);
                                if let Some(active_parameter) =
                                    active_parameter.and_then(|i| parameters.get(i as usize))
                                {
                                    match &active_parameter.label {
                                        ParameterLabelType::String(label) => {
                                            for (start, _) in signature_help.signatures[0]
                                                .label
                                                .match_indices(label.as_str())
                                            {
                                                if !signature_help.signatures[0].label.as_bytes()
                                                    [start + label.len()]
                                                .is_ascii_alphanumeric()
                                                {
                                                    active_parameter_range =
                                                        (start, start + label.len());
                                                }
                                            }
                                        }
                                        ParameterLabelType::Offsets(start, end) => {
                                            active_parameter_range =
                                                (*start as usize, *end as usize);
                                        }
                                    }
                                }
                                ui.text(
                                    &signature_help.signatures[0].label
                                        [0..active_parameter_range.0],
                                );
                                ui.same_line_with_spacing(0.0, 0.0);
                                ui.text_colored(
                                    [
                                        theme.active_parameter_color.r,
                                        theme.active_parameter_color.g,
                                        theme.active_parameter_color.b,
                                        1.0,
                                    ],
                                    &signature_help.signatures[0].label
                                        [active_parameter_range.0..active_parameter_range.1],
                                );
                                ui.same_line_with_spacing(0.0, 0.0);
                                ui.text(
                                    &signature_help.signatures[0].label[active_parameter_range.1..],
                                );
                            } else {
                                ui.text(&signature_help.signatures[0].label);
                            }
                        });
                }
            }
        }
    }
}

fn add_completions(ui: &Ui, theme: &Theme, font_size: (f32, f32), buffer: &mut Buffer) {
    if let Some(server) = &buffer.language_server {
        for (i, cursor) in buffer.cursors.iter_mut().enumerate() {
            let start_of_word = cursor
                .chars_until_pred_rev(&buffer.piece_table, |c| {
                    text_utils::char_type(c) != CharType::Word
                })
                .unwrap_or(0);
            if let Some(request) = cursor.completion_request.as_mut() {
                if let Some(completion_list) = server.borrow().saved_completions.get(&request.id) {
                    if completion_list.items.is_empty() {
                        continue;
                    }

                    let filtered_completions = get_filtered_completions(
                        &buffer.piece_table,
                        completion_list,
                        request,
                        cursor.position,
                    );

                    // Filter from start of word if manually triggered or
                    let request_position = if request.manually_triggered {
                        cursor.position.saturating_sub(start_of_word)
                    // Filter from start of request if triggered by a trigger character
                    } else {
                        request.initial_position
                    };

                    let (line, col) = (
                        buffer.piece_table.line_index(request_position),
                        buffer.piece_table.col_index(request_position),
                    );
                    let rect = line_col_to_rect(ui, line + 1, col, (1, 1), font_size);
                    let y_size = unsafe { ui.style().window_padding[1] }
                        + ui.text_line_height_with_spacing()
                            * 10.0f32.min(filtered_completions.len() as f32).min(
                                (ui.window_size()[1] - rect.Min.y)
                                    / ui.text_line_height_with_spacing(),
                            );
                    ui.window(format!("Completion {}", i))
                        .position(
                            [
                                rect.Min.x,
                                rect.Min.y + unsafe { ui.style().window_padding[1] },
                            ],
                            Condition::Always,
                        )
                        .size([-1.0, y_size], Condition::Always)
                        .no_inputs()
                        .no_decoration()
                        .movable(false)
                        .focused(false)
                        .focus_on_appearing(false)
                        .build(|| {
                            if ui.is_key_down(Key::LeftCtrl) && ui.is_key_pressed(Key::J) {
                                request.selection_index = min(
                                    request.selection_index + 1,
                                    filtered_completions.len().saturating_sub(1),
                                );
                            }
                            if ui.is_key_down(Key::LeftCtrl) && ui.is_key_pressed(Key::K) {
                                request.selection_index = request.selection_index.saturating_sub(1);
                            }

                            for (i, completion) in filtered_completions.iter().enumerate() {
                                if i == request.selection_index {
                                    ui.text(
                                        completion
                                            .insert_text
                                            .as_ref()
                                            .unwrap_or(&completion.label),
                                    );
                                    unsafe {
                                        igScrollToItem(ImGuiScrollFlags_None as i32);
                                    }
                                } else {
                                    ui.text_disabled(
                                        completion
                                            .insert_text
                                            .as_ref()
                                            .unwrap_or(&completion.label),
                                    );
                                }
                            }
                        });
                }
            }
        }
    }
}
