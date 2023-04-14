#![allow(dead_code)]
#![allow(unused_variables)]
#![feature(is_some_and)]
#![feature(iterator_try_collect)]
#![feature(pattern)]
#![feature(slice_take)]
#![feature(drain_filter)]
#![feature(let_chains)]

mod buffer;
mod cursor;
mod editor;
mod language_server;
mod language_server_types;
mod language_support;
mod piece_table;
mod renderer;
mod text_utils;
mod theme;
mod view;

pub enum DeviceInput {
    MouseWheel(isize),
}

use editor::Editor;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, ModifiersState, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Nimble")
        .with_inner_size(PhysicalSize::new(1920, 1080))
        .build(&event_loop)
        .unwrap();

    let mut editor = Editor::new(&window);
    // editor.open_file("C:/Users/Rasmus/Desktop/nimble/src/renderer.rs");
    editor.open_file("C:/VulkanSDK/1.3.239.0/Source/SPIRV-Reflect/spirv_reflect.c");

    let mut modifiers: Option<ModifiersState> = None;
    event_loop.run(move |event, _, control_flow| {
        if editor.update() {
            window.request_redraw();
        }

        match event {
            Event::RedrawRequested(_) => {
                editor.render();
            }
            Event::WindowEvent {
                event: WindowEvent::MouseWheel { delta, .. },
                ..
            } => {
                match delta {
                    MouseScrollDelta::LineDelta(_, lines) => {
                        editor.handle_input(DeviceInput::MouseWheel((lines as isize).signum()));
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        editor.handle_input(DeviceInput::MouseWheel((pos.y as isize).signum()));
                    }
                }
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::ReceivedCharacter(chr),
                ..
            } => {
                if !modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::CTRL)) {
                    editor.handle_char(chr);
                    window.request_redraw();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } => {
                if input.state == ElementState::Pressed {
                    if let Some(keycode) = input.virtual_keycode {
                        editor.handle_key(keycode, modifiers);
                        window.request_redraw();
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(modifiers_state),
                ..
            } => {
                modifiers = Some(modifiers_state);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                editor.shutdown();
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }
    });
}
