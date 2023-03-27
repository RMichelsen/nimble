use std::{collections::HashMap, str::pattern::Pattern};

use winit::window::Window;

use crate::{
    buffer::{Buffer, BufferCommand, CursorMotion},
    renderer::Renderer,
};

const NORMAL_MODE_COMMANDS: [&str; 12] =
    ["j", "k", "h", "l", "w", "b", "0", "$", "gg", "G", "x", "dd"];

enum EditorMode {
    Normal,
    Insert,
    Visual,
}

pub struct Editor {
    renderer: Renderer,
    buffers: HashMap<String, Buffer>,
    active_buffer: Option<String>,
    mode: EditorMode,
    input: String,
}

pub const SCROLL_LINES_PER_ROLL: isize = 3;
pub enum InputEvent {
    MouseWheel(isize),
}

impl Editor {
    pub fn new(window: &Window) -> Self {
        Self {
            renderer: Renderer::new(window),
            buffers: HashMap::default(),
            active_buffer: None,
            mode: EditorMode::Normal,
            input: String::default(),
        }
    }

    pub fn update(&self) {
        if let Some(buffer) = &self.active_buffer {
            self.renderer.draw_buffer(&self.buffers[buffer]);
        }
    }

    pub fn handle_input(&mut self, event: InputEvent) {
        if let Some(buffer) = Editor::active_buffer(&self.active_buffer, &mut self.buffers) {
            match event {
                InputEvent::MouseWheel(sign) => {
                    buffer.scroll_vertical(-sign * SCROLL_LINES_PER_ROLL)
                }
            }
        }
    }

    pub fn handle_char(&mut self, chr: char) {
        if let Some(buffer) = Editor::active_buffer(&self.active_buffer, &mut self.buffers) {
            self.input.push(chr);

            match (self.input.chars().next(), self.input.chars().nth(1)) {
                (Some('f'), Some(c)) => {
                    buffer.motion(CursorMotion::ForwardToChar(c as u8));
                    self.input.clear();
                    return;
                }
                (Some('F'), Some(c)) => {
                    buffer.motion(CursorMotion::BackwardToChar(c as u8));
                    self.input.clear();
                    return;
                }
                (Some('r'), Some(c)) => {
                    buffer.command(BufferCommand::ReplaceChar(c as u8));
                    self.input.clear();
                    return;
                }
                _ => (),
            }

            if !NORMAL_MODE_COMMANDS
                .iter()
                .any(|cmd| self.input.is_prefix_of(cmd))
            {
                self.input.clear();
                self.input.push(chr);
            }

            match self.input.as_str() {
                "j" => buffer.motion(CursorMotion::Down(1)),
                "k" => buffer.motion(CursorMotion::Up(1)),
                "h" => buffer.motion(CursorMotion::Backward(1)),
                "l" => buffer.motion(CursorMotion::Forward(1)),
                "w" => buffer.motion(CursorMotion::ForwardByWord),
                "b" => buffer.motion(CursorMotion::BackwardByWord),
                "0" => buffer.motion(CursorMotion::ToStartOfLine),
                "$" => buffer.motion(CursorMotion::ToEndOfLine),
                "gg" => buffer.motion(CursorMotion::ToStartOfFile),
                "G" => buffer.motion(CursorMotion::ToEndOfFile),
                "x" => buffer.command(BufferCommand::CutSelection),
                "dd" => buffer.command(BufferCommand::DeleteLine),
                "J" => buffer.command(BufferCommand::InsertCursorBelow),
                "K" => buffer.command(BufferCommand::InsertCursorAbove),
                _ => return,
            }
            self.input.clear();

            buffer.adjust_view();
        }
    }

    pub fn open_file(&mut self, path: &str) {
        if self.buffers.contains_key(path) {
            self.active_buffer = Some(path.to_string());
        } else {
            self.buffers.insert(
                path.to_string(),
                Buffer::new(path, self.renderer.num_rows, self.renderer.num_cols),
            );
            self.active_buffer = Some(path.to_string());
        }
    }

    fn active_buffer<'a>(
        active_buffer: &Option<String>,
        buffers: &'a mut HashMap<String, Buffer>,
    ) -> Option<&'a mut Buffer> {
        if let Some(buffer) = &active_buffer {
            buffers.get_mut(buffer)
        } else {
            None
        }
    }
}
