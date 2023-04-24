#![allow(dead_code)]
#![allow(unused_variables)]
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

#[cfg_attr(target_os = "windows", path = "graphics_context_windows.rs")]
#[cfg_attr(target_os = "macos", path = "graphics_context_macos.rs")]
mod graphics_context;

mod text_utils;
mod theme;
mod view;

pub enum DeviceInput {
    MouseWheel(isize),
}

use editor::Editor;
#[cfg(target_os = "macos")]
use objc::{msg_send, runtime::YES, sel, sel_impl};
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowExtMacOS;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, ModifiersState, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Nimble")
        .with_visible(false)
        .with_inner_size(LogicalSize::new(1920.0, 1080.0))
        .build(&event_loop)
        .unwrap();

    let mut editor = Editor::new(&window);
    editor.render();
    window.set_visible(true);

    editor.open_file("C:/Users/Rasmus/Desktop/nimble/src/buffer.rs");
    // editor.open_file("C:/VulkanSDK/1.3.239.0/Source/SPIRV-Reflect/spirv_reflect.c");
    request_redraw(&window);

    let mut modifiers: Option<ModifiersState> = None;
    event_loop.run(move |event, _, control_flow| {
        if editor.update() {
            editor.render();
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
                request_redraw(&window);
            }
            Event::WindowEvent {
                event: WindowEvent::ReceivedCharacter(chr),
                ..
            } => {
                if !modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::CTRL)) {
                    editor.handle_char(chr);
                    request_redraw(&window);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } => {
                if input.state == ElementState::Pressed {
                    if let Some(keycode) = input.virtual_keycode {
                        editor.handle_key(keycode, modifiers);
                        request_redraw(&window);
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

#[cfg(target_os = "macos")]
fn request_redraw(window: &Window) {
    let _: () = unsafe {
        msg_send![
            window.ns_view() as *mut objc::runtime::Object,
            setNeedsDisplay: YES
        ]
    };
}

#[cfg(target_os = "windows")]
fn request_redraw(window: &Window) {
    window.request_redraw();
}
