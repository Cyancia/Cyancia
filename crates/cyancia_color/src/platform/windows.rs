use anyhow::{Context, Result, anyhow, bail};
use gpui::Window;
use moxcms::ColorProfile;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_PATH_SOURCE_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
        QDC_VIRTUAL_MODE_AWARE, QueryDisplayConfig,
    },
    Foundation::HWND,
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromWindow},
    UI::ColorSystem::{
        CPST_STANDARD_DISPLAY_COLOR_MODE, CPT_ICC, ColorProfileGetDisplayDefault,
        GetColorDirectoryW, WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
    },
};

pub fn get_window_color_profile(window: &mut Window) -> Result<ColorProfile> {
    let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
        bail!("Unsupported window handle type")
    };

    let hwnd = HWND(handle.hwnd.get() as _);

    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_invalid() {
        bail!("MonitorFromWindow returned an invalid handle")
    }

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _) };
    if !ok.as_bool() {
        bail!("GetMonitorInfoW failed")
    }

    let (adapter_id, source_id) = find_path_for_device(&info.szDevice)?;

    let profile_path = unsafe {
        ColorProfileGetDisplayDefault(
            WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
            adapter_id,
            source_id,
            CPT_ICC,
            CPST_STANDARD_DISPLAY_COLOR_MODE,
        )
    };

    let profile_path = match profile_path {
        Ok(p) => p,
        // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
        Err(e) if e.code() == windows::core::HRESULT(0x80070002_u32 as i32) => {
            // The monitor has no associated color profile.
            return Ok(ColorProfile::new_srgb());
        }
        Err(e) => {
            return Err(e).context("ColorProfileGetDisplayDefault failed");
        }
    };

    let path_string = unsafe { profile_path.to_string() };

    if !profile_path.is_null() {
        // https://learn.microsoft.com/en-us/windows/win32/api/icm/nf-icm-colorprofilegetdisplaydefault#parameters
        // Receives a pointer to the default color profile name, which must be freed with LocalFree.
        unsafe {
            windows::Win32::Foundation::LocalFree(Some(std::mem::transmute::<
                *mut u16,
                windows::Win32::Foundation::HLOCAL,
            >(profile_path.as_ptr())));
        }
    }

    let resolved_path = resolve_profile_path(&path_string?)?;

    Ok(ColorProfile::new_from_slice(&std::fs::read(
        &resolved_path,
    )?)?)
}

fn resolve_profile_path(path: &str) -> Result<std::path::PathBuf> {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }

    let dir = get_color_directory()?;
    Ok(std::path::Path::new(&dir).join(path))
}

fn get_color_directory() -> Result<String> {
    let mut size = 0u32;
    unsafe {
        let _ = GetColorDirectoryW(None, None, &mut size);
    }
    if size == 0 {
        bail!("GetColorDirectoryW returned size 0")
    }

    let mut buf = vec![0u16; size as usize];
    let ok = unsafe {
        GetColorDirectoryW(
            None,
            Some(windows::core::PWSTR(buf.as_mut_ptr())),
            &mut size,
        )
    };
    if !ok.as_bool() {
        bail!("GetColorDirectoryW failed")
    }

    Ok(String::from_utf16(&buf)?.trim_matches('\0').to_string())
}

fn find_path_for_device(
    device_name: &[u16; 32],
) -> Result<(windows::Win32::Foundation::LUID, u32)> {
    let flags = QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE;

    let mut path_count = 0u32;
    let mut mode_count = 0u32;
    unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) }.ok()?;

    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![
        windows::Win32::Devices::Display::DISPLAYCONFIG_MODE_INFO::default();
        mode_count as usize
    ];

    unsafe {
        QueryDisplayConfig(
            flags,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    }
    .ok()?;

    let path = paths
        .iter()
        .find(|p| query_source_device_name(&p.sourceInfo).is_ok_and(|name| &name == device_name))
        .ok_or_else(|| anyhow!("Display path not found"))?;

    Ok((path.sourceInfo.adapterId, path.sourceInfo.id))
}

fn query_source_device_name(info: &DISPLAYCONFIG_PATH_SOURCE_INFO) -> Result<[u16; 32]> {
    let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
    name.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
    name.header.adapterId = info.adapterId;
    name.header.id = info.id;

    let status = unsafe { DisplayConfigGetDeviceInfo(&mut name.header) };

    if status != 0 {
        bail!("DisplayConfigGetDeviceInfo(GET_SOURCE_NAME) failed: {status}")
    }

    Ok(name.viewGdiDeviceName)
}
