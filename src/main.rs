#![allow(dead_code)]
#![allow(unused_variables)]
#![feature(is_some_and)]
#![feature(iterator_try_collect)]
#![feature(pattern)]

mod buffer;
mod cursor;
mod editor;
mod language_support;
mod renderer;
mod text_utils;
mod theme;
mod view;

use buffer::DeviceInput;
use editor::Editor;
use winit::{
    dpi::PhysicalSize,
    event::{Event, MouseScrollDelta, WindowEvent},
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
    editor.open_file("C:/Users/Rasmus/Desktop/nimble/src/renderer.rs");

    event_loop.run(move |event, _, control_flow| match event {
        Event::RedrawRequested(_) => {
            editor.update();
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
            editor.handle_char(chr);
            window.request_redraw();
        }
        Event::WindowEvent {
            event: WindowEvent::KeyboardInput { input, .. },
            ..
        } => {
            if let Some(keycode) = input.virtual_keycode {
                editor.handle_key(keycode);
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        _ => (),
    });
}
