use std::{collections::HashMap, str::pattern::Pattern};

use winit::window::Window;

use crate::{
    buffer::{Buffer, BufferCommand},
    renderer::Renderer,
};

const NORMAL_MODE_COMMANDS: [&str; 10] = ["j", "k", "h", "l", "w", "b", "0", "$", "gg", "G"];

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
            self.renderer.draw_buffer(&self.buffers[buffer].view);
        }
    }

    pub fn handle_input(&mut self, event: InputEvent) {
        if let Some(buffer) = Editor::active_buffer(&self.active_buffer, &mut self.buffers) {
            match event {
                InputEvent::MouseWheel(sign) => {
                    buffer.view.scroll_vertical(-sign * SCROLL_LINES_PER_ROLL)
                }
            }
        }
    }

    pub fn handle_char(&mut self, chr: char) {
        if let Some(buffer) = Editor::active_buffer(&self.active_buffer, &mut self.buffers) {
            self.input.push(chr);

            match (self.input.chars().next(), self.input.chars().nth(1)) {
                (Some('f'), Some(x)) => {
                    buffer.command(BufferCommand::MoveForwardToChar(x as u8));
                    self.input.clear();
                    return;
                }
                (Some('F'), Some(x)) => {
                    buffer.command(BufferCommand::MoveBackwardToChar(x as u8));
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
                "j" => buffer.command(BufferCommand::MoveDown(1)),
                "k" => buffer.command(BufferCommand::MoveUp(1)),
                "h" => buffer.command(BufferCommand::MoveBackward(1)),
                "l" => buffer.command(BufferCommand::MoveForward(1)),
                "w" => buffer.command(BufferCommand::MoveForwardByWord),
                "b" => buffer.command(BufferCommand::MoveBackwardByWord),
                "0" => buffer.command(BufferCommand::MoveToStartOfLine),
                "$" => buffer.command(BufferCommand::MoveToEndOfLine),
                "gg" => buffer.command(BufferCommand::MoveToStartOfFile),
                "G" => buffer.command(BufferCommand::MoveToEndOfFile),
                _ => return,
            }
            self.input.clear();

            buffer.view.adjust();
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
