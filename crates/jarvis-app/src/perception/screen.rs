#[derive(Debug, Clone, Default)]
pub struct ScreenContext {
    pub window_title: String,
    pub is_browser: bool,
    pub is_media: bool,
}

impl ScreenContext {
    pub fn is_empty(&self) -> bool {
        self.window_title.is_empty()
    }
}

pub fn get_active_window() -> Option<ScreenContext> {
    #[cfg(target_os = "windows")]
    {
        get_active_window_win32()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn get_active_window_win32() -> Option<ScreenContext> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    unsafe {
        let hwnd = winapi::um::winuser::GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }

        let mut buf = [0u16; 512];
        let len = winapi::um::winuser::GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len <= 0 {
            return None;
        }

        let title = OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .to_string();

        let is_browser = ["Chrome", "Firefox", "Edge", "Safari", "Opera", "Brave"]
            .iter()
            .any(|b| title.contains(b));
        let is_media = ["VLC", "YouTube", "Spotify", "Media Player", "mpv", "Winamp", "iTunes"]
            .iter()
            .any(|m| title.contains(m));

        Some(ScreenContext { window_title: title, is_browser, is_media })
    }
}
