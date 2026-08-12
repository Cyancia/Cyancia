use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW};

pub fn set_window_parent(parent: u64, child: u64) {
    unsafe {
        SetWindowLongPtrW(
            HWND(child as *mut std::ffi::c_void),
            GWLP_HWNDPARENT,
            parent as isize,
        );
    }
}
