use winit::{window::Window};

pub fn open_folder(window: &Window) -> Option<String> {
    None
}

pub struct PlatformResources {
}

impl PlatformResources {
    pub fn new(window: &Window) -> Self {
        Self {
        }
    }

    pub fn set_clipboard(&self, text: &[u8]) {
    }
    pub fn get_clipboard(&self) -> Vec<u8> {
        vec![]
    }
    pub fn confirm_quit(&self, path: &str) -> Option<bool> {
    	None
    }
}
