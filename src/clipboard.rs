//! Clipboard use for secrets. On Windows, copies also ask Clipboard History and Cloud Clipboard
//! to skip the content. Other programs can still read the clipboard while it holds the secret.

use gpui::{App, ClipboardItem};

pub fn copy_secret(cx: &mut App, text: &str) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    #[cfg(windows)]
    win::exclude_from_history();
}

/// Clear the clipboard only if it still holds `text`, so later copies by the user stay intact.
pub fn clear_if_holds(cx: &mut App, text: &str) {
    if cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .as_deref()
        == Some(text)
    {
        #[cfg(windows)]
        if win::clear() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
    }
}

#[cfg(windows)]
mod win {
    use std::{thread::sleep, time::Duration};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };
    use windows_sys::w;

    fn open() -> bool {
        for _ in 0..10 {
            // SAFETY: plain Win32 call; a null owner window is allowed.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return true;
            }
            sleep(Duration::from_millis(10));
        }
        false
    }

    pub fn clear() -> bool {
        if !open() {
            return false;
        }
        // SAFETY: the clipboard is open.
        let ok = unsafe { EmptyClipboard() } != 0;
        unsafe { CloseClipboard() };
        ok
    }

    /// Add the documented opt-out formats next to the text that is already on the clipboard.
    pub fn exclude_from_history() {
        if !open() {
            return;
        }
        let formats = [
            w!("CanIncludeInClipboardHistory"),
            w!("CanUploadToCloudClipboard"),
            w!("ExcludeClipboardContentFromMonitorProcessing"),
        ];
        for name in formats {
            // SAFETY: the clipboard is open, `name` is a valid NUL terminated UTF-16 string, and
            // the memory handed to SetClipboardData is moveable, sized for one DWORD, and owned
            // by the system after a successful call.
            unsafe {
                let format = RegisterClipboardFormatW(name);
                let handle = GlobalAlloc(GMEM_MOVEABLE, 4);
                if format == 0 || handle.is_null() {
                    continue;
                }
                let ptr = GlobalLock(handle) as *mut u32;
                if ptr.is_null() {
                    GlobalFree(handle);
                    continue;
                }
                ptr.write(0);
                GlobalUnlock(handle);
                if SetClipboardData(format, handle).is_null() {
                    GlobalFree(handle);
                }
            }
        }
        // SAFETY: the clipboard is open.
        unsafe { CloseClipboard() };
    }
}
