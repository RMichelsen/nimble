use std::ffi::{c_char, c_long};

use objc::{
    class, msg_send,
    runtime::{Object, Sel, NO, YES},
    sel, sel_impl,
};
use winit::window::Window;

extern "C" {
    pub static NSPasteboardTypeString: Sel;
}

pub fn open_folder(window: &Window) -> Option<String> {
    let panel: *mut Object = unsafe { msg_send![class!(NSOpenPanel), openPanel] };
    let _: () = unsafe { msg_send![panel, setCanChooseFiles: NO] };
    let _: () = unsafe { msg_send![panel, setCanChooseDirectories: YES] };
    let _: () = unsafe { msg_send![panel, setAllowsMultipleSelection: NO] };
    let _: () = unsafe { msg_send![panel, runModal] };
    let url: *mut Object = unsafe { msg_send![panel, URL] };
    let path: *mut Object = unsafe { msg_send![url, path] };
    let bytes = unsafe {
        let bytes: *const c_char = msg_send![path, UTF8String];
        bytes as *const u8
    };
    let len = unsafe { msg_send![path, lengthOfBytesUsingEncoding:4] };
    Some(
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytes, len)) }
            .to_string(),
    )
}

pub struct PlatformResources {}

impl PlatformResources {
    pub fn new(window: &Window) -> Self {
        Self {}
    }

    pub fn set_clipboard(&self, text: &[u8]) {
        let clipboard: *mut Object = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };

        unsafe {
            let string: *mut Object = msg_send![class!(NSString), alloc];
            let allocated_string: *mut Object =
                msg_send![string, initWithBytes:text.as_ptr() length:text.len() encoding:4];
            let _: () = msg_send![clipboard, clearContents];
            let _: () =
                msg_send![clipboard, setString:allocated_string forType:NSPasteboardTypeString];
        }
    }

    pub fn get_clipboard(&self) -> Vec<u8> {
        unsafe {
            let clipboard: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
            let string: *mut Object = msg_send![clipboard, stringForType: NSPasteboardTypeString];
            let bytes: *const c_char = msg_send![string, UTF8String];
            let len = msg_send![string, lengthOfBytesUsingEncoding:4];
            std::slice::from_raw_parts(bytes as *mut u8, len).to_vec()
        }
    }
    pub fn confirm_quit(&self, path: &str) -> Option<bool> {
        println!("Confirming Quit!");
        unsafe {
            let panel: *mut Object = msg_send![class!(NSAlert), new];

            let prompt = format!("Do you want to save changes to {} before quitting?", path);
            let title = "Save changes?";
            let yes = "Yes";
            let no = "No";
            let cancel = "Cancel";

            let prompt_string: *mut Object = msg_send![class!(NSString), alloc];
            let prompt_allocated_string: *mut Object = msg_send![prompt_string, initWithBytes:prompt.as_ptr() length:prompt.len() encoding:4];

            let title_string: *mut Object = msg_send![class!(NSString), alloc];
            let title_allocated_string: *mut Object =
                msg_send![title_string, initWithBytes:title.as_ptr() length:title.len() encoding:4];

            let yes_string: *mut Object = msg_send![class!(NSString), alloc];
            let yes_allocated_string: *mut Object =
                msg_send![yes_string, initWithBytes:yes.as_ptr() length:yes.len() encoding:4];

            let no_string: *mut Object = msg_send![class!(NSString), alloc];
            let no_allocated_string: *mut Object =
                msg_send![no_string, initWithBytes:no.as_ptr() length:no.len() encoding:4];

            let cancel_string: *mut Object = msg_send![class!(NSString), alloc];
            let cancel_allocated_string: *mut Object = msg_send![cancel_string, initWithBytes:cancel.as_ptr() length:cancel.len() encoding:4];

            let _: () = msg_send![panel, setMessageText: title_allocated_string];
            let _: () = msg_send![panel, setInformativeText: prompt_allocated_string];
            let _: () = msg_send![panel, addButtonWithTitle: yes_allocated_string];
            let _: () = msg_send![panel, addButtonWithTitle: no_allocated_string];
            let _: () = msg_send![panel, addButtonWithTitle: cancel_allocated_string];
            let response: c_long = msg_send![panel, runModal];
            match response {
                1000 => Some(true),
                1001 => Some(false),
                _ => None,
            }
        }
    }
}
