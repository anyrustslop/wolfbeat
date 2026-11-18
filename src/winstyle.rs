//! Windows 11 touches for Wolfbeat's custom (title-bar-less) window: rounded corners, dark frame and shadow.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE};
use windows::Win32::UI::Controls::MARGINS;
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn style_window(w: &impl HasWindowHandle) {
    let Ok(handle) = w.window_handle() else { return };
    let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
    let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
    unsafe {
        let round: u32 = 2; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const u32 as *const _, 4);
        let dark: u32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const u32 as *const _, 4);
        // A 1px frame extension brings back the native drop shadow on a borderless window.
        let m = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
    }
}

/// Local hour (0–23) for the "Good morning / evening" greeting.
pub fn local_hour() -> u16 {
    unsafe { windows::Win32::System::SystemInformation::GetLocalTime().wHour }
}

/// Puts Wolfbeat in a Windows job: the hidden player (started by Wolfbeat) joins it automatically,
/// so Windows treats them as one app, and the player is always closed with Wolfbeat (even on a crash).
pub fn join_job() {
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let Ok(job) = CreateJobObjectW(None, windows::core::PCWSTR::null()) else { return };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let _ = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        let _ = AssignProcessToJobObject(job, GetCurrentProcess());
        // The job handle is intentionally kept open for Wolfbeat's whole lifetime.
    }
}
