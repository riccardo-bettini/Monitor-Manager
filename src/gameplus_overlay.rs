//! Instant click-through native Win32 Layered Overlay (`WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOPMOST`)
//! backing ASUS DisplayWidget Center's GamePlus features (Crosshair, Timer, FPS Counter, Display Alignment)
//! on any monitor even when fullscreen-borderless games or third-party panels are active.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

const SW_SHOWNA: i32 = 8;
const SW_HIDE: i32 = 0;
const ULW_ALPHA: u32 = 0x0000_0002;
const AC_SRC_OVER: u8 = 0;
const AC_SRC_ALPHA: u8 = 1;
const DIB_RGB_COLORS: u32 = 0;
const BI_RGB: u32 = 0;
const PM_REMOVE: u32 = 0x0001;
const WM_QUIT: u32 = 0x0012;
const HWND_TOPMOST: isize = -1;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;

#[repr(C)]
#[derive(Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct BlendFunction {
    blend_op: u8,
    blend_flags: u8,
    source_constant_alpha: u8,
    alpha_format: u8,
}

#[repr(C)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    bmi_header: BitmapInfoHeader,
    bmi_colors: [u32; 1],
}

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: extern "system" fn(isize, u32, usize, isize) -> isize,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: isize,
    h_icon: isize,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: isize,
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct DwmTimingInfo {
    cb_size: u32,
    rate_refresh_num: u32,
    rate_refresh_den: u32,
    qpc_refresh_period: u64,
    rate_compose_num: u32,
    rate_compose_den: u32,
    qpc_vblank: u64,
    c_refresh: u64,
    c_dx_refresh: u32,
    qpc_compose: u64,
    c_frame: u64,
    c_dx_present: u32,
    c_refresh_frame: u64,
    c_frame_submitted: u64,
    c_dx_present_submitted: u32,
    c_frame_confirmed: u64,
    c_dx_present_confirmed: u32,
    c_refresh_confirmed: u64,
    c_dx_refresh_confirmed: u32,
    c_frames_late: u64,
    c_frames_outstanding: u32,
    c_frame_displayed: u64,
    qpc_frame_displayed: u64,
    c_refresh_frame_displayed: u64,
    c_frame_complete: u64,
    qpc_frame_complete: u64,
    c_frame_pending: u64,
    qpc_frame_pending: u64,
    c_frames_displayed: u64,
    c_frames_complete: u64,
    c_frames_pending: u64,
    c_frames_dropped: u64,
    c_frames_missed: u64,
    c_refresh_next_displayed: u64,
    c_refresh_next_presented: u64,
    c_refreshes_displayed: u64,
    c_refreshes_presented: u64,
    c_refresh_started: u64,
    c_pixels_received: u64,
    c_pixels_drawn: u64,
    c_buffers_empty: u64,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(lpwcx: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        dw_ex_style: u32,
        lp_class_name: *const u16,
        lp_window_name: *const u16,
        dw_style: u32,
        x: i32,
        y: i32,
        n_width: i32,
        n_height: i32,
        h_wnd_parent: isize,
        h_menu: isize,
        h_instance: isize,
        lp_param: *const c_void,
    ) -> isize;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn DefWindowProcW(hwnd: isize, msg: u32, w_param: usize, l_param: isize) -> isize;
    fn ShowWindow(hwnd: isize, n_cmd_show: i32) -> i32;
    fn PeekMessageW(lp_msg: *mut Msg, hwnd: isize, w_msg_filter_min: u32, w_msg_filter_max: u32, w_remove_msg: u32) -> i32;
    fn TranslateMessage(lp_msg: *const Msg) -> i32;
    fn DispatchMessageW(lp_msg: *const Msg) -> isize;
    fn UpdateLayeredWindow(
        hwnd: isize,
        hdc_dst: isize,
        ppt_dst: *const Point,
        psize: *const Size,
        hdc_src: isize,
        ppt_src: *const Point,
        cr_key: u32,
        pblend: *const BlendFunction,
        dw_flags: u32,
    ) -> i32;
    fn SetWindowPos(
        hwnd: isize,
        hwnd_insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn DeleteDC(hdc: isize) -> i32;
    fn SelectObject(hdc: isize, h: isize) -> isize;
    fn DeleteObject(ho: isize) -> i32;
    fn CreateDIBSection(
        hdc: isize,
        pbmi: *const BitmapInfo,
        usage: u32,
        ppv_bits: *mut *mut c_void,
        h_section: isize,
        offset: u32,
    ) -> isize;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetCompositionTimingInfo(hwnd: isize, p_timing_info: *mut DwmTimingInfo) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> isize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayConfig {
    pub enabled: bool,
    pub target_rect: [i32; 4], // [left, top, right, bottom]
    pub crosshair_style: u32,  // 0=Off, 1=Punto blu, 2=Dot verde, 3=Mini Duplex blu, 4=Mini Duplex verde, 5=Heavy Duplex blu, 6=Heavy Duplex verde
    pub timer_mode: u32,       // 0=Off, 1=30m, 2=40m, 3=50m, 4=60m, 5=90m, 6=Stopwatch
    pub timer_paused: bool,
    pub fps_counter: bool,
    pub alignment_grid: bool,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_rect: [0, 0, 1920, 1080],
            crosshair_style: 0,
            timer_mode: 0,
            timer_paused: false,
            fps_counter: false,
            alignment_grid: false,
            offset_x: 0,
            offset_y: 0,
        }
    }
}

impl OverlayConfig {
    pub fn has_any_active(&self) -> bool {
        self.enabled
            && (self.crosshair_style > 0
                || self.timer_mode > 0
                || self.fps_counter
                || self.alignment_grid)
    }
}

#[derive(Clone)]
pub struct GamePlusOverlayEngine {
    shared: Arc<Mutex<OverlayShared>>,
}

struct OverlayShared {
    config: OverlayConfig,
    timer_epoch: Instant,
    timer_duration_secs: u64,
    timer_elapsed_when_paused: u64,
    dirty: bool,
    current_fps_display: u32,
}

impl GamePlusOverlayEngine {
    pub fn spawn() -> Self {
        let shared = Arc::new(Mutex::new(OverlayShared {
            config: OverlayConfig::default(),
            timer_epoch: Instant::now(),
            timer_duration_secs: 0,
            timer_elapsed_when_paused: 0,
            dirty: false,
            current_fps_display: 60,
        }));
        let worker_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("gameplus-overlay".into())
            .spawn(move || overlay_thread(worker_shared))
            .ok();
        Self { shared }
    }

    pub fn update_config(&self, cfg: OverlayConfig) {
        if let Ok(mut g) = self.shared.lock() {
            let timer_changed = g.config.timer_mode != cfg.timer_mode;
            let pause_changed = g.config.timer_paused != cfg.timer_paused;
            g.config = cfg.clone();
            if timer_changed {
                g.timer_epoch = Instant::now();
                g.timer_elapsed_when_paused = 0;
                g.timer_duration_secs = match cfg.timer_mode {
                    1 => 30 * 60,
                    2 => 40 * 60,
                    3 => 50 * 60,
                    4 => 60 * 60,
                    5 => 90 * 60,
                    _ => 0,
                };
            } else if pause_changed {
                if cfg.timer_paused {
                    g.timer_elapsed_when_paused += g.timer_epoch.elapsed().as_secs();
                } else {
                    g.timer_epoch = Instant::now();
                }
            }
            g.dirty = true;
        }
    }

    pub fn reset_timer(&self) {
        if let Ok(mut g) = self.shared.lock() {
            g.timer_epoch = Instant::now();
            g.timer_elapsed_when_paused = 0;
            g.dirty = true;
        }
    }
}

extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn overlay_thread(shared: Arc<Mutex<OverlayShared>>) {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let cls = to_wide_null("MonitorManagerGamePlusOverlay");
        let wnd_name = to_wide_null("GamePlus HUD");
        let wc = WndClassExW {
            cb_size: std::mem::size_of::<WndClassExW>() as u32,
            style: 0,
            lpfn_wnd_proc: wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: hinst,
            h_icon: 0,
            h_cursor: 0,
            hbr_background: 0,
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: cls.as_ptr(),
            h_icon_sm: 0,
        };
        RegisterClassExW(&wc);

        let ex_style =
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let hwnd = CreateWindowExW(
            ex_style,
            cls.as_ptr(),
            wnd_name.as_ptr(),
            WS_POPUP,
            0,
            0,
            100,
            100,
            0,
            0,
            hinst,
            std::ptr::null(),
        );
        if hwnd == 0 {
            return;
        }

        let mut last_tick = Instant::now();
        let mut last_dwm_frames: u64 = 0;
        let mut last_dwm_sample = Instant::now();
        let mut visible = false;

        let mut dib_w = 0i32;
        let mut dib_h = 0i32;
        let mut hbmp = 0isize;
        let mut bits: *mut u32 = std::ptr::null_mut();

        loop {
            let mut msg: Msg = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    if hbmp != 0 {
                        DeleteObject(hbmp);
                    }
                    DestroyWindow(hwnd);
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let snapshot = {
                let mut g = shared.lock().unwrap();
                if g.config.fps_counter && last_dwm_sample.elapsed() >= Duration::from_millis(450) {
                    let mut ti: DwmTimingInfo = std::mem::zeroed();
                    ti.cb_size = std::mem::size_of::<DwmTimingInfo>() as u32;
                    if DwmGetCompositionTimingInfo(0, &mut ti) == 0 {
                        let dt = last_dwm_sample.elapsed().as_secs_f64();
                        let cur_frames = ti.c_frames_displayed;
                        if last_dwm_frames > 0 && cur_frames >= last_dwm_frames && dt > 0.05 {
                            let fps = ((cur_frames - last_dwm_frames) as f64 / dt).round() as u32;
                            let max_hz = if ti.rate_refresh_den > 0 {
                                (ti.rate_refresh_num / ti.rate_refresh_den).max(30)
                            } else {
                                180
                            };
                            g.current_fps_display = fps.clamp(1, max_hz);
                        } else if ti.rate_refresh_den > 0 {
                            g.current_fps_display = ti.rate_refresh_num / ti.rate_refresh_den;
                        }
                        last_dwm_frames = cur_frames;
                        last_dwm_sample = Instant::now();
                        g.dirty = true;
                    }
                }

                let need_timer_tick =
                    g.config.timer_mode > 0 && last_tick.elapsed() >= Duration::from_millis(500);
                if g.dirty || (g.config.has_any_active() && need_timer_tick) {
                    g.dirty = false;
                    let elapsed = g.timer_elapsed_when_paused
                        + if g.config.timer_paused {
                            0
                        } else {
                            g.timer_epoch.elapsed().as_secs()
                        };
                    Some((
                        g.config.clone(),
                        g.timer_duration_secs,
                        elapsed,
                        g.current_fps_display,
                    ))
                } else {
                    None
                }
            };

            if let Some((cfg, timer_total, timer_elapsed, fps_val)) = snapshot {
                last_tick = Instant::now();
                if !cfg.has_any_active() {
                    if visible {
                        ShowWindow(hwnd, SW_HIDE);
                        visible = false;
                    }
                } else {
                    let w = (cfg.target_rect[2] - cfg.target_rect[0]).max(320);
                    let h = (cfg.target_rect[3] - cfg.target_rect[1]).max(240);
                    if w != dib_w || h != dib_h || hbmp == 0 {
                        if hbmp != 0 {
                            DeleteObject(hbmp);
                        }
                        let screen_dc = GetDC(0);
                        let mut bmi: BitmapInfo = std::mem::zeroed();
                        bmi.bmi_header.bi_size = std::mem::size_of::<BitmapInfoHeader>() as u32;
                        bmi.bmi_header.bi_width = w;
                        bmi.bmi_header.bi_height = -h; // top-down
                        bmi.bmi_header.bi_planes = 1;
                        bmi.bmi_header.bi_bit_count = 32;
                        bmi.bmi_header.bi_compression = BI_RGB;
                        let mut raw_ptr: *mut c_void = std::ptr::null_mut();
                        hbmp = CreateDIBSection(
                            screen_dc,
                            &bmi,
                            DIB_RGB_COLORS,
                            &mut raw_ptr,
                            0,
                            0,
                        );
                        ReleaseDC(0, screen_dc);
                        bits = raw_ptr.cast::<u32>();
                        dib_w = w;
                        dib_h = h;
                    }

                    if hbmp != 0 && !bits.is_null() {
                        let buf = std::slice::from_raw_parts_mut(bits, (w * h) as usize);
                        render_overlay_frame(buf, w, h, &cfg, timer_total, timer_elapsed, fps_val);

                        let screen_dc = GetDC(0);
                        let mem_dc = CreateCompatibleDC(screen_dc);
                        let old = SelectObject(mem_dc, hbmp);
                        let pt_dst = Point {
                            x: cfg.target_rect[0],
                            y: cfg.target_rect[1],
                        };
                        let sz = Size { cx: w, cy: h };
                        let pt_src = Point { x: 0, y: 0 };
                        let bf = BlendFunction {
                            blend_op: AC_SRC_OVER,
                            blend_flags: 0,
                            source_constant_alpha: 255,
                            alpha_format: AC_SRC_ALPHA,
                        };
                        SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            cfg.target_rect[0],
                            cfg.target_rect[1],
                            w,
                            h,
                            SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                        UpdateLayeredWindow(
                            hwnd,
                            screen_dc,
                            &pt_dst,
                            &sz,
                            mem_dc,
                            &pt_src,
                            0,
                            &bf,
                            ULW_ALPHA,
                        );
                        SelectObject(mem_dc, old);
                        DeleteDC(mem_dc);
                        ReleaseDC(0, screen_dc);
                        if !visible {
                            ShowWindow(hwnd, SW_SHOWNA);
                            visible = true;
                        }
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(60));
        }
    }
}

/// Premultiplied ARGB color helper (`0xAARRGGBB`).
#[inline]
fn argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    let af = a as u32;
    let pr = ((r as u32) * af + 127) / 255;
    let pg = ((g as u32) * af + 127) / 255;
    let pb = ((b as u32) * af + 127) / 255;
    (af << 24) | (pr << 16) | (pg << 8) | pb
}

#[inline]
fn put_px(buf: &mut [u32], w: i32, h: i32, x: i32, y: i32, color: u32) {
    if x >= 0 && y >= 0 && x < w && y < h {
        buf[(y * w + x) as usize] = color;
    }
}

fn fill_rect(buf: &mut [u32], w: i32, h: i32, x0: i32, y0: i32, rw: i32, rh: i32, color: u32) {
    let x_start = x0.max(0);
    let y_start = y0.max(0);
    let x_end = (x0 + rw).min(w);
    let y_end = (y0 + rh).min(h);
    for y in y_start..y_end {
        let row = (y * w) as usize;
        for x in x_start..x_end {
            buf[row + x as usize] = color;
        }
    }
}

fn draw_rect_outlined(buf: &mut [u32], w: i32, h: i32, x: i32, y: i32, rw: i32, rh: i32, fill: u32, rim: u32) {
    fill_rect(buf, w, h, x - 1, y - 1, rw + 2, rh + 2, rim);
    fill_rect(buf, w, h, x, y, rw, rh, fill);
}

fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        ':' => [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        ' ' => [0; 7],
        _ => [0; 7],
    }
}

fn draw_text_5x7(buf: &mut [u32], w: i32, h: i32, x0: i32, y0: i32, scale: i32, text: &str, color: u32) {
    let mut cx = x0;
    for ch in text.chars() {
        let g = glyph_5x7(ch);
        for (ry, row) in g.iter().enumerate() {
            for rx in 0..5 {
                if (row >> (4 - rx)) & 1 == 1 {
                    fill_rect(
                        buf,
                        w,
                        h,
                        cx + rx * scale,
                        y0 + (ry as i32) * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cx += 6 * scale;
    }
}

fn render_overlay_frame(
    buf: &mut [u32],
    w: i32,
    h: i32,
    cfg: &OverlayConfig,
    timer_total: u64,
    timer_elapsed: u64,
    fps_val: u32,
) {
    buf.fill(0);
    let rim = argb(215, 10, 12, 16);

    // 1. Display Alignment Grid (screen edges + center cross ticks)
    if cfg.alignment_grid {
        let guide = argb(230, 232, 194, 132);
        let dark = argb(200, 12, 14, 18);
        // Center edge ticks (left, right, top, bottom)
        for dy in -18i32..=18 {
            if dy.abs() % 6 == 0 {
                draw_rect_outlined(buf, w, h, 8, h / 2 + dy * 4, 28, 3, guide, dark);
                draw_rect_outlined(buf, w, h, w - 36, h / 2 + dy * 4, 28, 3, guide, dark);
            }
        }
        for dx in -18i32..=18 {
            if dx.abs() % 6 == 0 {
                draw_rect_outlined(buf, w, h, w / 2 + dx * 4, 8, 3, 28, guide, dark);
                draw_rect_outlined(buf, w, h, w / 2 + dx * 4, h - 36, 3, 28, guide, dark);
            }
        }
        // Center horizon & vertical crosshair alignment lines
        draw_rect_outlined(buf, w, h, w / 2 - 48, h / 2 - 1, 96, 2, guide, dark);
        draw_rect_outlined(buf, w, h, w / 2 - 1, h / 2 - 48, 2, 96, guide, dark);
    }

    // 2. Crosshair (matching Screenshot 3 styles: Blue Dot, Green Dot, Mini Duplex Blue/Green, Heavy Duplex Blue/Green)
    if cfg.crosshair_style > 0 {
        let cx = (w / 2 + cfg.offset_x).clamp(24, w - 24);
        let cy = (h / 2 + cfg.offset_y).clamp(24, h - 24);
        let blue = argb(255, 0, 168, 255);
        let green = argb(255, 0, 240, 88);

        match cfg.crosshair_style {
            1 => {
                // Punto blu
                draw_rect_outlined(buf, w, h, cx - 3, cy - 3, 6, 6, blue, rim);
            }
            2 => {
                // Dot verde
                draw_rect_outlined(buf, w, h, cx - 3, cy - 3, 6, 6, green, rim);
            }
            3 | 4 => {
                // Mini Duplex blu / verde
                let c = if cfg.crosshair_style == 3 { blue } else { green };
                let gap = 5;
                let len = 9;
                let thick = 2;
                draw_rect_outlined(buf, w, h, cx - 1, cy - 1, 3, 3, c, rim);
                draw_rect_outlined(buf, w, h, cx - gap - len, cy - 1, len, thick, c, rim);
                draw_rect_outlined(buf, w, h, cx + gap, cy - 1, len, thick, c, rim);
                draw_rect_outlined(buf, w, h, cx - 1, cy - gap - len, thick, len, c, rim);
                draw_rect_outlined(buf, w, h, cx - 1, cy + gap, thick, len, c, rim);
            }
            5 | 6 => {
                // Heavy Duplex blu / verde
                let c = if cfg.crosshair_style == 5 { blue } else { green };
                let gap = 6;
                let len = 14;
                let thick = 4;
                draw_rect_outlined(buf, w, h, cx - 2, cy - 2, 4, 4, c, rim);
                draw_rect_outlined(buf, w, h, cx - gap - len, cy - 2, len, thick, c, rim);
                draw_rect_outlined(buf, w, h, cx + gap, cy - 2, len, thick, c, rim);
                draw_rect_outlined(buf, w, h, cx - 2, cy - gap - len, thick, len, c, rim);
                draw_rect_outlined(buf, w, h, cx - 2, cy + gap, thick, len, c, rim);
            }
            _ => {}
        }
    }

    // 3. HUD Badge (Timer and/or FPS Counter in top-left/top-right with D-pad offset support)
    let hud_x = (36 + cfg.offset_x).clamp(12, w - 260);
    let mut hud_y = (36 + cfg.offset_y).clamp(12, h - 140);
    let badge_bg = argb(210, 18, 20, 26);
    let badge_border = argb(240, 212, 168, 108);
    let text_gold = argb(255, 240, 204, 148);
    let text_green = argb(255, 86, 245, 138);

    if cfg.timer_mode > 0 {
        let secs = if cfg.timer_mode == 6 {
            timer_elapsed
        } else {
            timer_total.saturating_sub(timer_elapsed)
        };
        let mins = secs / 60;
        let rem = secs % 60;
        let txt = format!("{:02}:{:02}", mins.min(999), rem);
        let bw = (txt.len() as i32) * 18 + 28;
        let bh = 38;
        fill_rect(buf, w, h, hud_x - 1, hud_y - 1, bw + 2, bh + 2, badge_border);
        fill_rect(buf, w, h, hud_x, hud_y, bw, bh, badge_bg);
        draw_text_5x7(buf, w, h, hud_x + 14, hud_y + 9, 3, &txt, text_gold);
        hud_y += bh + 10;
    }

    if cfg.fps_counter {
        let txt = format!("{} FPS", fps_val);
        let bw = (txt.len() as i32) * 18 + 26;
        let bh = 38;
        fill_rect(buf, w, h, hud_x - 1, hud_y - 1, bw + 2, bh + 2, badge_border);
        fill_rect(buf, w, h, hud_x, hud_y, bw, bh, badge_bg);
        draw_text_5x7(buf, w, h, hud_x + 13, hud_y + 9, 3, &txt, text_green);
    }

    let _ = put_px;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_renders_non_empty_pixels_for_active_modes() {
        let cfg = OverlayConfig {
            enabled: true,
            target_rect: [0, 0, 640, 480],
            crosshair_style: 5,
            timer_mode: 1,
            timer_paused: false,
            fps_counter: true,
            alignment_grid: true,
            offset_x: 0,
            offset_y: 0,
        };
        assert!(cfg.has_any_active());
        let mut buf = vec![0u32; 640 * 480];
        render_overlay_frame(&mut buf, 640, 480, &cfg, 1800, 42, 180);
        assert!(buf.iter().any(|&px| px != 0));
    }
}
