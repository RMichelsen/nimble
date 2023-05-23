#![allow(dead_code)]
#![allow(unused_variables)]
#![feature(iterator_try_collect)]
#![feature(pattern)]
#![feature(slice_take)]
#![feature(drain_filter)]
#![feature(byte_slice_trim_ascii)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(if_let_guard)]

mod buffer;
mod cursor;
mod editor;
mod language_server;
mod language_server_types;
mod language_support;
mod piece_table;
mod renderer;
mod syntect;
mod text_utils;
mod theme;
mod view;

#[cfg_attr(target_os = "windows", path = "graphics_context_windows.rs")]
#[cfg_attr(target_os = "macos", path = "graphics_context_macos.rs")]
mod graphics_context;

#[cfg_attr(target_os = "windows", path = "platform_resources_windows.rs")]
#[cfg_attr(target_os = "macos", path = "platform_resources_macos.rs")]
mod platform_resources;

use std::time::{Duration, Instant};

use editor::Editor;
#[cfg(target_os = "macos")]
use objc::{msg_send, runtime::YES, sel, sel_impl};
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowExtMacOS;
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, Event, ModifiersState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::EventLoop,
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
    editor.render(&window);
    window.set_visible(true);

    request_redraw(&window);

    let mut modifiers: Option<ModifiersState> = None;
    let mut mouse_position: Option<PhysicalPosition<f64>> = None;
    let mut left_mouse_button_state: Option<ElementState> = None;
    let mut left_mouse_button_timer = Instant::now();
    let mut double_click_timer = Instant::now();
    let mut hover_timer = Some(Instant::now());
    event_loop.run(move |event, _, control_flow| {
        editor.update_layouts(&window);

        // Handle incoming responses, re-render if necessary
        if editor.handle_lsp_responses() {
            editor.render(&window);
        }

        if editor.update_highlights() {
            request_redraw(&window);
        }

        match event {
            Event::RedrawRequested(_) => {
                editor.render(&window);
            }
            Event::WindowEvent {
                event: WindowEvent::MouseWheel { delta, .. },
                ..
            } => {
                match delta {
                    MouseScrollDelta::LineDelta(_, lines) => {
                        if let Some(position) = mouse_position {
                            editor.handle_scroll(
                                position.to_logical(window.scale_factor()),
                                (lines as isize).signum(),
                                &window,
                            );
                        };
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        if let Some(position) = mouse_position {
                            editor.handle_scroll(
                                position.to_logical(window.scale_factor()),
                                (pos.y as isize).signum(),
                                &window,
                            );
                        }
                    }
                }
                request_redraw(&window);
            }
            Event::WindowEvent {
                event: WindowEvent::ReceivedCharacter(chr),
                ..
            } => {
                if !modifiers.is_some_and(|modifiers| modifiers.contains(ModifiersState::CTRL)) {
                    if !editor.handle_char(&window, chr) {
                        editor.lsp_shutdown();
                        control_flow.set_exit();
                    }
                    request_redraw(&window);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } => {
                if input.state == ElementState::Pressed {
                    if let Some(key_code) = input.virtual_keycode {
                        if !editor.handle_key(&window, key_code, modifiers) {
                            editor.lsp_shutdown();
                            control_flow.set_exit();
                        }
                        request_redraw(&window);
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                if button == MouseButton::Left {
                    left_mouse_button_state = Some(state);
                    if state == ElementState::Pressed {
                        if let Some(position) = mouse_position {
                            if left_mouse_button_timer.elapsed() < Duration::from_millis(500) {
                                if editor.handle_mouse_double_click(
                                    position.to_logical(window.scale_factor()),
                                    modifiers,
                                    &window,
                                ) {
                                    double_click_timer = Instant::now();
                                }
                            } else {
                                editor.handle_mouse_pressed(
                                    position.to_logical(window.scale_factor()),
                                    modifiers,
                                    &window,
                                );
                            }
                        }
                        left_mouse_button_timer = Instant::now();
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
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                let old_position = mouse_position;
                mouse_position = Some(position);

                if let Some(old_position) = old_position {
                    if editor.has_moved_cell(
                        old_position.to_logical(window.scale_factor()),
                        position.to_logical(window.scale_factor()),
                    ) {
                        if editor.hovering() {
                            request_redraw(&window);
                        }
                        hover_timer = Some(Instant::now());
                        editor.handle_mouse_exit_hover();
                    }
                }

                if let Some(state) = left_mouse_button_state {
                    if state == ElementState::Pressed
                        && double_click_timer.elapsed() > Duration::from_millis(200)
                    {
                        editor.handle_mouse_drag(
                            position.to_logical(window.scale_factor()),
                            modifiers,
                        );
                        request_redraw(&window);
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if editor.ready_to_quit() {
                    editor.lsp_shutdown();
                    control_flow.set_exit();
                }
            }
            _ => (),
        }

        if let Some(mouse_position) = mouse_position {
            if let Some(timer) = hover_timer {
                if timer.elapsed() > Duration::from_millis(300) {
                    editor.handle_mouse_hover(
                        mouse_position.to_logical(window.scale_factor()),
                        &window,
                    );
                    hover_timer = None;
                    request_redraw(&window);
                }
            }
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
