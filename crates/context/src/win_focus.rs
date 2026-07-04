use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

use crate::{AppInfo, ContextProvider, TextContext};

/// Windows focus + text context via Win32 and UI Automation.
pub struct WindowsContext;

impl ContextProvider for WindowsContext {
    fn focused_app(&self) -> Option<AppInfo> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return None;
            }
            let mut title_buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut title_buf).max(0) as usize;
            let title = String::from_utf16_lossy(&title_buf[..len]);

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut process = String::new();
            if pid != 0 {
                if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    let mut buf = [0u16; 1024];
                    let mut size = buf.len() as u32;
                    if QueryFullProcessImageNameW(
                        handle,
                        PROCESS_NAME_WIN32,
                        PWSTR(buf.as_mut_ptr()),
                        &mut size,
                    )
                    .is_ok()
                    {
                        let full = String::from_utf16_lossy(&buf[..size as usize]);
                        process = full
                            .rsplit(['\\', '/'])
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                    }
                    let _ = CloseHandle(handle);
                }
            }
            Some(AppInfo { process, title })
        }
    }

    fn text_context(&self) -> TextContext {
        crate::win_uia::capture_text_context(150)
    }
}
