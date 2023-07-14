#![windows_subsystem = "windows"]
#![allow(dead_code)]
#![allow(unused_variables)]
#![feature(iterator_try_collect)]
#![feature(pattern)]
#![feature(slice_take)]
#![feature(extract_if)]
#![feature(byte_slice_trim_ascii)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(if_let_guard)]
#![feature(split_array)]
#![feature(int_roundings)]

mod buffer;
mod cursor;
mod editor;
mod language_server;
mod language_server_types;
mod language_support;
mod piece_table;
mod platform_resources;
mod renderer;
mod syntect;
mod text_utils;
mod theme;
mod user_interface;

use std::time::{Duration, Instant};

use editor::Editor;
use imgui_winit_support::winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use renderer::Renderer;
use theme::THEMES;
use user_interface::UserInterface;

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Nimble")
        .with_visible(false)
        .with_inner_size(PhysicalSize::new(2560.0, 1440.0))
        .build(&event_loop)
        .unwrap();

    let mut theme = THEMES[0];
    let mut user_interface = UserInterface::new(&window, &theme);
    let mut editor = Editor::new(&window);
    let renderer = Renderer::new(&window, &user_interface.font_atlas_texture());

    let mut last_frame = Instant::now();
    let mut highlight_timer = Instant::now();
    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(_) => {
            let now = Instant::now();
            user_interface.pre_frame(now - last_frame);
            last_frame = now;
        }
        Event::MainEventsCleared => {
            user_interface.prepare_frame(&window);
        }
        Event::RedrawEventsCleared => {
            editor.handle_lsp_responses();
            if let Some(render_data) =
                user_interface.run(&window, &renderer, &mut editor, &mut theme)
            {
                if highlight_timer.elapsed() > Duration::from_micros(8333) {
                    editor.update_highlights(&render_data);
                    highlight_timer = Instant::now();
                }
                unsafe {
                    renderer.draw(&theme, &editor.buffers, &render_data);
                }
                window.set_visible(true);
            } else {
                control_flow.set_exit();
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        event => {
            user_interface.handle_event(&window, &event);
        }
    });
}
