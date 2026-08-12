use anyhow::{Result, bail};
use moxcms::ColorProfile;

#[cfg(target_os = "windows")]
mod windows;

/// Returns the color profile of the display the window is on.
///
/// `raw_window_id` is the platform window id (e.g. the raw `HWND` value on
/// Windows, as returned by `iced::window::raw_id`).
#[inline]
#[allow(unused)]
pub fn get_window_color_profile(raw_window_id: u64) -> Result<ColorProfile> {
    #[cfg(target_os = "windows")]
    return windows::get_window_color_profile(raw_window_id);

    bail!("Unsupported platform")
}
