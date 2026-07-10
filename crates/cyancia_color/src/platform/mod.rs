use anyhow::{Result, bail};
use gpui::Window;
use moxcms::ColorProfile;

#[cfg(target_os = "windows")]
mod windows;

// bail! is unreachable on supported platforms
#[inline]
#[allow(unused)]
pub fn get_window_color_profile(window: &mut Window) -> Result<ColorProfile> {
    #[cfg(target_os = "windows")]
    return windows::get_window_color_profile(window);

    bail!("Unsupported platform")
}
