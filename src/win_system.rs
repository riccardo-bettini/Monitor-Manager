//! Windows OS integrations equivalent to ASUS DisplayWidget Center:
//! - Desktop Taskbar Auto-Hide (`SHAppBarMessage`)
//! - Windows System & App Light/Dark Theme (`HKCU\...\Themes\Personalize` + `ImmersiveColorSet` broadcast)
//! - Native File Open / Save Dialogs (`GetOpenFileNameW` / `GetSaveFileNameW`)
//! - MultiScreen Window Enumeration, Monitor Work-Area Queries & Clean DWM-Adjusted Window Snapping
//! - Active Foreground Process Name Lookup (for App Tweaker)

use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::path::PathBuf;

const HKEY_CURRENT_USER: isize = -0x7FFF_FFFF; // 0x80000001u32 as isize
const KEY_READ: u32 = 0x20019;
const KEY_SET_VALUE: u32 = 0x0002;
const REG_DWORD: u32 = 4;

const ABM_GETSTATE: u32 = 0x0000_0004;
const ABM_SETSTATE: u32 = 0x0000_000A;
const ABS_AUTOHIDE: isize = 0x0000_0001;
const ABS_ALWAYSONTOP: isize = 0x0000_0002;

const HWND_BROADCAST: isize = 0xFFFF;
const WM_SETTINGCHANGE: u32 = 0x001A;
const SMTO_ABORTIFHUNG: u32 = 0x0002;

const SW_RESTORE: i32 = 9;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const DWMWA_CLOAKED: u32 = 14;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
const OFN_OVERWRITEPROMPT: u32 = 0x0000_0002;
const OFN_NOCHANGEDIR: u32 = 0x0000_0008;

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    dw_flags: u32,
}

#[repr(C)]
struct AppBarData {
    cb_size: u32,
    hwnd: isize,
    u_callback_message: u32,
    u_edge: u32,
    rc: Rect,
    l_param: isize,
}

#[repr(C)]
struct OpenFileNameW {
    l_struct_size: u32,
    hwnd_owner: isize,
    h_instance: isize,
    lpstr_filter: *const u16,
    lpstr_custom_filter: *mut u16,
    n_max_cust_filter: u32,
    n_filter_index: u32,
    lpstr_file: *mut u16,
    n_max_file: u32,
    lpstr_file_title: *mut u16,
    n_max_file_title: u32,
    lpstr_initial_dir: *const u16,
    lpstr_title: *const u16,
    flags: u32,
    n_file_offset: u16,
    n_file_extension: u16,
    lpstr_def_ext: *const u16,
    l_cust_data: isize,
    lpfn_hook: isize,
    lp_template_name: *const u16,
    pv_reserved: *mut c_void,
    dw_reserved: u32,
    flags_ex: u32,
}

#[link(name = "shell32")]
extern "system" {
    fn SHAppBarMessage(dw_message: u32, p_data: *mut AppBarData) -> usize;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        h_key: isize,
        lp_sub_key: *const u16,
        ul_options: u32,
        sam_desired: u32,
        phk_result: *mut isize,
    ) -> i32;
    fn RegQueryValueExW(
        h_key: isize,
        lp_value_name: *const u16,
        lp_reserved: *mut u32,
        lp_type: *mut u32,
        lp_data: *mut u8,
        lpcb_data: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        h_key: isize,
        lp_value_name: *const u16,
        reserved: u32,
        dw_type: u32,
        lp_data: *const u8,
        cb_data: u32,
    ) -> i32;
    fn RegCloseKey(h_key: isize) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
    fn SendMessageTimeoutW(
        h_wnd: isize,
        msg: u32,
        w_param: usize,
        l_param: isize,
        fu_flags: u32,
        u_timeout: u32,
        lpdw_result: *mut usize,
    ) -> isize;
    fn EnumWindows(lp_enum_func: extern "system" fn(isize, isize) -> i32, l_param: isize) -> i32;
    fn IsWindowVisible(h_wnd: isize) -> i32;
    fn IsIconic(h_wnd: isize) -> i32;
    fn IsZoomed(h_wnd: isize) -> i32;
    fn ShowWindow(h_wnd: isize, n_cmd_show: i32) -> i32;
    fn GetWindowTextW(h_wnd: isize, lp_string: *mut u16, n_max_count: i32) -> i32;
    fn GetWindowTextLengthW(h_wnd: isize) -> i32;
    fn GetWindowLongPtrW(h_wnd: isize, n_index: i32) -> isize;
    fn GetWindowRect(h_wnd: isize, lp_rect: *mut Rect) -> i32;
    fn SetWindowPos(
        h_wnd: isize,
        h_wnd_insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(h_wnd: isize, lpdw_process_id: *mut u32) -> u32;
    fn EnumDisplayMonitors(
        hdc: isize,
        lprc_clip: *const Rect,
        lpfn_enum: extern "system" fn(isize, isize, *mut Rect, isize) -> i32,
        dw_data: isize,
    ) -> i32;
    fn GetMonitorInfoW(h_monitor: isize, lpmi: *mut MonitorInfo) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(
        hwnd: isize,
        dw_attribute: u32,
        pv_attribute: *mut c_void,
        cb_attribute: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32) -> isize;
    fn QueryFullProcessImageNameW(
        h_process: isize,
        dw_flags: u32,
        lp_exe_name: *mut u16,
        lpdw_size: *mut u32,
    ) -> i32;
    fn CloseHandle(h_object: isize) -> i32;
    fn GetCurrentProcessId() -> u32;
}

#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(lpofn: *mut OpenFileNameW) -> i32;
    fn GetSaveFileNameW(lpofn: *mut OpenFileNameW) -> i32;
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads whether the Windows desktop taskbar is set to auto-hide.
pub fn get_taskbar_autohide() -> bool {
    unsafe {
        let mut abd: AppBarData = std::mem::zeroed();
        abd.cb_size = std::mem::size_of::<AppBarData>() as u32;
        let state = SHAppBarMessage(ABM_GETSTATE, &mut abd);
        (state & (ABS_AUTOHIDE as usize)) != 0
    }
}

/// Immediately sets Windows desktop taskbar auto-hide state.
pub fn set_taskbar_autohide(enable: bool) -> Result<(), String> {
    unsafe {
        let class = to_wide_null("Shell_TrayWnd");
        let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
        let mut abd: AppBarData = std::mem::zeroed();
        abd.cb_size = std::mem::size_of::<AppBarData>() as u32;
        abd.hwnd = hwnd;
        let current = SHAppBarMessage(ABM_GETSTATE, &mut abd) as isize;
        let always_on_top = if (current & ABS_ALWAYSONTOP) != 0 {
            ABS_ALWAYSONTOP
        } else {
            ABS_ALWAYSONTOP
        };
        abd.l_param = if enable {
            ABS_AUTOHIDE | always_on_top
        } else {
            always_on_top
        };
        SHAppBarMessage(ABM_SETSTATE, &mut abd);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsThemeState {
    pub system_light: bool,
    pub apps_light: bool,
}

const PERSONALIZE_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

/// Reads Windows System & App Light/Dark mode (`SystemUsesLightTheme` / `AppsUseLightTheme`).
pub fn get_windows_theme() -> WindowsThemeState {
    let mut system_light = false;
    let mut apps_light = false;
    unsafe {
        let subkey = to_wide_null(PERSONALIZE_SUBKEY);
        let mut hkey = 0isize;
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            for (name, target) in [
                ("SystemUsesLightTheme", &mut system_light),
                ("AppsUseLightTheme", &mut apps_light),
            ] {
                let vname = to_wide_null(name);
                let mut val: u32 = 0;
                let mut size: u32 = 4;
                let mut kind: u32 = 0;
                if RegQueryValueExW(
                    hkey,
                    vname.as_ptr(),
                    std::ptr::null_mut(),
                    &mut kind,
                    (&mut val as *mut u32).cast::<u8>(),
                    &mut size,
                ) == 0
                {
                    *target = val != 0;
                }
            }
            RegCloseKey(hkey);
        }
    }
    WindowsThemeState {
        system_light,
        apps_light,
    }
}

/// Immediately updates Windows System & App Light/Dark theme and broadcasts `ImmersiveColorSet`.
pub fn set_windows_theme(system_light: bool, apps_light: bool) -> Result<(), String> {
    unsafe {
        let subkey = to_wide_null(PERSONALIZE_SUBKEY);
        let mut hkey = 0isize;
        let rc = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_READ | KEY_SET_VALUE,
            &mut hkey,
        );
        if rc != 0 {
            return Err(format!("RegOpenKeyExW Personalize failed ({rc})"));
        }

        let sys_val: u32 = if system_light { 1 } else { 0 };
        let app_val: u32 = if apps_light { 1 } else { 0 };
        let sys_name = to_wide_null("SystemUsesLightTheme");
        let app_name = to_wide_null("AppsUseLightTheme");

        RegSetValueExW(
            hkey,
            sys_name.as_ptr(),
            0,
            REG_DWORD,
            (&sys_val as *const u32).cast::<u8>(),
            4,
        );
        RegSetValueExW(
            hkey,
            app_name.as_ptr(),
            0,
            REG_DWORD,
            (&app_val as *const u32).cast::<u8>(),
            4,
        );
        RegCloseKey(hkey);

        let param = to_wide_null("ImmersiveColorSet");
        let mut res = 0usize;
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            param.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            300,
            &mut res,
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SnapWindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub exe_name: String,
    pub rect: [i32; 4],
}

/// Returns active foreground window `.exe` filename (lowercase) + window title for App Tweaker / AI Visual.
pub fn get_foreground_app() -> Option<(String, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            return None;
        }
        let exe = process_exe_name_for_hwnd(hwnd)?;
        let len = GetWindowTextLengthW(hwnd);
        let title = if len > 0 {
            let mut buf = vec![0u16; (len + 2) as usize];
            let read = GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
            String::from_utf16_lossy(&buf[..read.max(0) as usize])
        } else {
            String::new()
        };
        Some((exe, title))
    }
}

unsafe fn process_exe_name_for_hwnd(hwnd: isize) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 {
        return None;
    }
    let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if hproc == 0 {
        return None;
    }
    let mut buf = vec![0u16; 512];
    let mut size = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(hproc, 0, buf.as_mut_ptr(), &mut size);
    CloseHandle(hproc);
    if ok == 0 || size == 0 {
        return None;
    }
    let full = String::from_utf16_lossy(&buf[..size as usize]);
    let name = PathBuf::from(full)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Lists open, non-cloaked top-level user application windows cleanly.
pub fn enumerate_open_windows() -> Vec<SnapWindowInfo> {
    struct Ctx {
        own_pid: u32,
        list: Vec<SnapWindowInfo>,
    }

    extern "system" fn enum_proc(hwnd: isize, lparam: isize) -> i32 {
        unsafe {
            let ctx = &mut *(lparam as *mut Ctx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if (ex_style & WS_EX_TOOLWINDOW) != 0 {
                return 1;
            }
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast::<c_void>(),
                4,
            );
            if cloaked != 0 {
                return 1;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 || pid == ctx.own_pid {
                return 1;
            }
            let tlen = GetWindowTextLengthW(hwnd);
            if tlen <= 0 {
                return 1;
            }
            let mut buf = vec![0u16; (tlen + 2) as usize];
            let got = GetWindowTextW(hwnd, buf.as_mut_ptr(), tlen + 1);
            let title = String::from_utf16_lossy(&buf[..got.max(0) as usize]);
            if title.is_empty() || title == "Program Manager" {
                return 1;
            }
            let exe_name = process_exe_name_for_hwnd(hwnd).unwrap_or_else(|| "app.exe".into());
            let mut rc = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetWindowRect(hwnd, &mut rc);
            ctx.list.push(SnapWindowInfo {
                hwnd,
                title,
                exe_name,
                rect: [rc.left, rc.top, rc.right, rc.bottom],
            });
            1
        }
    }

    let mut ctx = Ctx {
        own_pid: unsafe { GetCurrentProcessId() },
        list: Vec::new(),
    };
    unsafe {
        EnumWindows(enum_proc, (&mut ctx as *mut Ctx) as isize);
    }
    ctx.list
}

/// Returns monitor `[left, top, right, bottom]` (`work_area` and `full_bounds`) by monitor display index (`0..N`).
pub fn get_monitor_rects(monitor_index: usize) -> Option<([i32; 4], [i32; 4])> {
    struct Ctx {
        items: Vec<([i32; 4], [i32; 4])>,
    }

    extern "system" fn mon_proc(
        hmon: isize,
        _hdc: isize,
        _lprc: *mut Rect,
        dw_data: isize,
    ) -> i32 {
        unsafe {
            let ctx = &mut *(dw_data as *mut Ctx);
            let mut mi: MonitorInfo = std::mem::zeroed();
            mi.cb_size = std::mem::size_of::<MonitorInfo>() as u32;
            if GetMonitorInfoW(hmon, &mut mi) != 0 {
                let work = [
                    mi.rc_work.left,
                    mi.rc_work.top,
                    mi.rc_work.right,
                    mi.rc_work.bottom,
                ];
                let full = [
                    mi.rc_monitor.left,
                    mi.rc_monitor.top,
                    mi.rc_monitor.right,
                    mi.rc_monitor.bottom,
                ];
                ctx.items.push((work, full));
            }
            1
        }
    }

    let mut ctx = Ctx { items: Vec::new() };
    unsafe {
        EnumDisplayMonitors(0, std::ptr::null(), mon_proc, (&mut ctx as *mut Ctx) as isize);
    }
    ctx.items.get(monitor_index).copied().or_else(|| ctx.items.first().copied())
}

/// MultiScreen grid zone definition computed from a monitor's work area `[left, top, right, bottom]`.
pub fn compute_multiscreen_zones(work: [i32; 4], layout_id: &str) -> Vec<[i32; 4]> {
    let [l, t, r, b] = work;
    let w = (r - l).max(100);
    let h = (b - t).max(100);
    match layout_id {
        "cols_2" => vec![[l, t, l + w / 2, b], [l + w / 2, t, r, b]],
        "cols_3" => vec![
            [l, t, l + w / 3, b],
            [l + w / 3, t, l + (2 * w) / 3, b],
            [l + (2 * w) / 3, t, r, b],
        ],
        "rows_2" => vec![[l, t, r, t + h / 2], [l, t + h / 2, r, b]],
        "focus_1_2" => {
            let mw = (w * 6) / 10;
            vec![
                [l, t, l + mw, b],
                [l + mw, t, r, t + h / 2],
                [l + mw, t + h / 2, r, b],
            ]
        }
        "grid_2x2" => vec![
            [l, t, l + w / 2, t + h / 2],
            [l + w / 2, t, r, t + h / 2],
            [l, t + h / 2, l + w / 2, b],
            [l + w / 2, t + h / 2, r, b],
        ],
        _ => vec![[l, t, r, b]],
    }
}

/// Snaps a window to `[left, top, right, bottom]` while compensating for invisible Win10/11 DWM drop-shadow borders.
pub fn snap_window_to_rect(hwnd: isize, target: [i32; 4]) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe {
        if IsIconic(hwnd) != 0 || IsZoomed(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }

        let mut win_rc = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut ext_rc = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut pad_l = 0;
        let mut pad_t = 0;
        let mut pad_r = 0;
        let mut pad_b = 0;

        if GetWindowRect(hwnd, &mut win_rc) != 0
            && DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut ext_rc as *mut Rect).cast::<c_void>(),
                std::mem::size_of::<Rect>() as u32,
            ) == 0
        {
            pad_l = ext_rc.left - win_rc.left;
            pad_t = ext_rc.top - win_rc.top;
            pad_r = win_rc.right - ext_rc.right;
            pad_b = win_rc.bottom - ext_rc.bottom;
        }

        let x = target[0] - pad_l;
        let y = target[1] - pad_t;
        let cx = (target[2] - target[0]) + pad_l + pad_r;
        let cy = (target[3] - target[1]) + pad_t + pad_b;
        SetWindowPos(hwnd, 0, x, y, cx.max(120), cy.max(120), SWP_NOZORDER | SWP_NOACTIVATE) != 0
    }
}

/// Shows a native Windows File Open dialog and returns the selected file path.
pub fn pick_open_file(title: &str, filter_pairs: &[(&str, &str)]) -> Option<PathBuf> {
    run_file_dialog(false, title, "", filter_pairs)
}

/// Shows a native Windows File Save dialog and returns the chosen path.
pub fn pick_save_file(
    title: &str,
    default_filename: &str,
    filter_pairs: &[(&str, &str)],
) -> Option<PathBuf> {
    run_file_dialog(true, title, default_filename, filter_pairs)
}

fn run_file_dialog(
    save: bool,
    title: &str,
    default_filename: &str,
    filter_pairs: &[(&str, &str)],
) -> Option<PathBuf> {
    let mut filter_w: Vec<u16> = Vec::new();
    for (desc, pat) in filter_pairs {
        filter_w.extend(desc.encode_utf16());
        filter_w.push(0);
        filter_w.extend(pat.encode_utf16());
        filter_w.push(0);
    }
    filter_w.push(0);

    let mut file_buf = vec![0u16; 1024];
    for (i, ch) in default_filename.encode_utf16().take(file_buf.len() - 1).enumerate() {
        file_buf[i] = ch;
    }

    let title_w = to_wide_null(title);
    let def_ext = to_wide_null("json");

    unsafe {
        let mut ofn: OpenFileNameW = std::mem::zeroed();
        ofn.l_struct_size = std::mem::size_of::<OpenFileNameW>() as u32;
        ofn.lpstr_filter = filter_w.as_ptr();
        ofn.n_filter_index = 1;
        ofn.lpstr_file = file_buf.as_mut_ptr();
        ofn.n_max_file = file_buf.len() as u32;
        ofn.lpstr_title = title_w.as_ptr();
        ofn.lpstr_def_ext = def_ext.as_ptr();
        ofn.flags = if save {
            OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR
        } else {
            OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR
        };

        let ok = if save {
            GetSaveFileNameW(&mut ofn)
        } else {
            GetOpenFileNameW(&mut ofn)
        };
        if ok == 0 {
            return None;
        }
        let end = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
        let s = String::from_utf16_lossy(&file_buf[..end]);
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_multiscreen_zones_cleanly() {
        let work = [0, 0, 2560, 1400];
        let cols2 = compute_multiscreen_zones(work, "cols_2");
        assert_eq!(cols2, vec![[0, 0, 1280, 1400], [1280, 0, 2560, 1400]]);
        let focus = compute_multiscreen_zones(work, "focus_1_2");
        assert_eq!(focus.len(), 3);
        assert_eq!(focus[0][2], 1536);
        assert_eq!(focus[1][3], 700);
        assert_eq!(focus[2][1], 700);
    }

    #[test]
    fn windows_queries_do_not_panic() {
        let _ = get_taskbar_autohide();
        let _ = get_windows_theme();
        let _ = get_foreground_app();
        let _ = enumerate_open_windows();
    }
}
