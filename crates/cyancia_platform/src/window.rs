pub fn set_window_parent(parent: u64, child: u64) {
    if cfg!(target_os = "windows") {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW};

        unsafe {
            SetWindowLongPtrW(
                HWND(child as *mut std::ffi::c_void),
                GWLP_HWNDPARENT,
                parent as isize,
            );
        }
    } else {
        // TODO Linux wayland and MacOS
        log::error!(
            "set_window_parent is not supported on {}",
            std::env::consts::OS
        );
    }
}
