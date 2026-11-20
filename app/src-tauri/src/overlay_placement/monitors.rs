use super::{Bounds, Monitor};
use std::{io, mem::size_of};
use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{
            DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC,
            HMONITOR, MONITORINFO, MONITORINFOEXW,
        },
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::{EDD_GET_DEVICE_INTERFACE_NAME, MONITORINFOF_PRIMARY},
        },
    },
    core::{BOOL, PCWSTR},
};

pub(super) fn connected() -> io::Result<Vec<Monitor>> {
    let mut monitors = Vec::new();
    // The callback is synchronous; Windows keeps no reference to this vector.
    let data = LPARAM((&mut monitors as *mut Vec<Monitor>) as isize);
    if !unsafe { EnumDisplayMonitors(None, None, Some(collect), data) }.as_bool() {
        return Err(io::Error::other("Cannot enumerate monitor work areas"));
    }
    Ok(monitors)
}

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    // cbSize requests the extended structure, so pass a pointer to the entire allocation.
    if !unsafe { GetMonitorInfoW(monitor, std::ptr::from_mut(&mut info).cast()) }.as_bool() {
        return BOOL(0);
    }
    let work = info.monitorInfo.rcWork;
    if work.right <= work.left || work.bottom <= work.top {
        return BOOL(1);
    }
    // The interface path identifies the output even when Windows renumbers DISPLAYn.
    let mut device = DISPLAY_DEVICEW {
        cb: size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    let found = unsafe {
        EnumDisplayDevicesW(
            PCWSTR(info.szDevice.as_ptr()),
            0,
            &mut device,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    };
    let name = if found.as_bool() && device.DeviceID[0] != 0 {
        &device.DeviceID[..]
    } else {
        &info.szDevice[..]
    };
    let id =
        String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())]);
    let (mut dpi_x, mut dpi_y) = (96, 96);
    let dpi = match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
    {
        Ok(()) => dpi_x.max(1),
        Err(_) => 96,
    };
    let monitors = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
    monitors.push(Monitor {
        id,
        work: Bounds {
            x: work.left,
            y: work.top,
            width: work.right.saturating_sub(work.left),
            height: work.bottom.saturating_sub(work.top),
        },
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        dpi,
    });
    BOOL(1)
}
