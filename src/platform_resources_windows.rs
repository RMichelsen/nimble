use std::{ffi::CStr, ptr::copy_nonoverlapping};

use windows::{
    w,
    Win32::{
        Foundation::{HANDLE, HGLOBAL, HWND},
        System::{
            Com::{CoCreateInstance, CLSCTX_ALL},
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
            },
            Memory::{GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock, GMEM_ZEROINIT},
        },
        UI::{
            Shell::{FileOpenDialog, IFileOpenDialog, FOS_PICKFOLDERS, SIGDN_FILESYSPATH},
            WindowsAndMessaging::{MessageBoxW, IDNO, IDYES, MB_YESNOCANCEL},
        },
    },
};
use winit::{platform::windows::WindowExtWindows, window::Window};

pub fn open_folder() -> Option<String> {
    unsafe {
        let file_dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;

        file_dialog.SetOptions(FOS_PICKFOLDERS).ok()?;
        file_dialog.Show(None).ok()?;

        if let Ok(result) = file_dialog.GetResult() {
            return result
                .GetDisplayName(SIGDN_FILESYSPATH)
                .unwrap()
                .to_string()
                .ok();
        }
    };

    None
}

pub struct PlatformResources {
    hwnd: HWND,
}

impl PlatformResources {
    pub fn new(window: &Window) -> Self {
        Self {
            hwnd: HWND(window.hwnd()),
        }
    }

    pub fn set_clipboard(&self, text: &[u8]) {
        unsafe {
            if OpenClipboard(self.hwnd).into() {
                if EmptyClipboard().into() {
                    if let Ok(data) = GlobalAlloc(GMEM_ZEROINIT, text.len() + 1) {
                        let memory = GlobalLock(data);
                        if memory.is_null() {
                            GlobalFree(data).unwrap();
                            return;
                        }
                        copy_nonoverlapping(text.as_ptr(), data.0 as *mut _, text.len());

                        // Clipboard format CF_TEXT = 1
                        if SetClipboardData(1, HANDLE(data.0)).is_err() {
                            GlobalFree(data).unwrap();
                        }
                        GlobalUnlock(data);
                    }
                }
                CloseClipboard();
            }
        }
    }

    pub fn get_clipboard(&self) -> Vec<u8> {
        unsafe {
            if OpenClipboard(self.hwnd).into() {
                // Clipboard format CF_TEXT = 1
                if let Ok(data) = GetClipboardData(1) {
                    let memory = GlobalLock(HGLOBAL(data.0));
                    let content = CStr::from_ptr(memory as *mut _).to_bytes().into();
                    GlobalUnlock(HGLOBAL(data.0));
                    CloseClipboard();
                    return content;
                }

                CloseClipboard();
            }
        }

        vec![]
    }

    pub fn confirm_quit(&self, path: &str) -> Option<bool> {
        unsafe {
            match MessageBoxW(
                self.hwnd,
                w!("Save changes?"),
                w!("Do you want to save changes before quitting?"),
                MB_YESNOCANCEL,
            ) {
                IDYES => Some(true),
                IDNO => Some(false),
                _ => None,
            }
        }
    }
}
