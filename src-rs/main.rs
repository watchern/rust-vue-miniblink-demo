// Windows GUI 子系统：双击 exe 直接启动，不再弹控制台窗口。
// 日志统一写 logs/mbvue.log（见 log 模块）。
#![windows_subsystem = "windows"]

mod ffi;
mod i18n;
mod ipc;
mod log;
mod server;

pub(crate) use log::logln;

use std::os::raw::c_char;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, Ordering};
use std::sync::OnceLock;

use serde_json::{json, Value};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging as win;

const WS_POPUP: u32 = 0x8000_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
// 有它窗口才有 resize 边框：WS_POPUP 窗口默认不可拖拽缩放，
// 无边框 UI 靠 miniblink 的 ncHittestPaddingWidth 做边缘命中测试。
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_EX_APPWINDOW: u32 = 0x0004_0000;

const WIN_W: i32 = 1180;
const WIN_H: i32 = 760;
// 最小尺寸（WM_GETMINMAXINFO 的 ptMinTrackSize），配合 WS_THICKFRAME 生效；
// ipc.rs 的 win.resize 钳制也复用这两个常量
pub(crate) const WIN_MIN_W: i32 = 900;
pub(crate) const WIN_MIN_H: i32 = 600;

const GWL_WNDPROC: i32 = -4;
const WM_GETMINMAXINFO: u32 = 0x0024;

// ---- 自定义窗口消息 ----
const WM_TRAY: u32 = 0x8000 + 1; // 托盘图标回调
const WM_APP_MINIMIZE: u32 = 0x8000 + 2; // 收进托盘
const WM_APP_EXIT: u32 = 0x8000 + 3; // 退出进程
const ID_TRAY_OPEN: u32 = 1001;
const ID_TRAY_EXIT: u32 = 1002;
const SC_MINIMIZE: u32 = 0xF020;

/// 子类化前的原窗口过程（miniblink 自己装的）
static PREV_WND_PROC: AtomicIsize = AtomicIsize::new(0);

/// 宿主窗口句柄（on_document_ready 的尺寸保险要用）
static WINDOW_HWND: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// 托盘图标是否已添加（避免重复删除）
static TRAY_ADDED: AtomicBool = AtomicBool::new(false);
static TRAY_HWND: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// 运行时数据目录（解析一次后缓存；默认 exe 同级，Program Files 不可写时回退 LOCALAPPDATA）
static USER_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 子类化窗口过程：
///   · WM_GETMINMAXINFO  —— 限制最小跟踪尺寸
///   · WM_SYSCOMMAND(SC_MINIMIZE) / WM_APP_MINIMIZE —— 收进托盘（隐藏窗口）
///   · WM_APP_EXIT / WM_DESTROY —— 退出进程
///   · WM_TRAY —— 托盘右键菜单 / 双击恢复
///   · WM_EXITSIZEMOVE —— 记住窗口尺寸
///   其余消息透传给 miniblink 原过程。
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        win::WM_SYSCOMMAND => {
            // 系统命令里的"最小化"（任务栏按钮 / Win+M 等）也收进托盘
            if (wparam & 0xFFF0) == SC_MINIMIZE as usize {
                win::ShowWindow(hwnd, 0); // SW_HIDE
                crate::logln!("[tray] 最小化 → 收进托盘");
                return 0;
            }
        }
        WM_APP_MINIMIZE => {
            win::ShowWindow(hwnd, 0); // SW_HIDE
            return 0;
        }
        WM_APP_EXIT => {
            win::DestroyWindow(hwnd);
            return 0;
        }
        win::WM_DESTROY => {
            remove_tray();
            let mb = ffi::mb();
            (mb.mbExitMessageLoop)();
            win::PostQuitMessage(0);
            return 0;
        }
        WM_TRAY => match lparam as u32 {
            0x0205 | 0x007B => show_tray_menu(hwnd), // WM_RBUTTONUP / WM_CONTEXTMENU
            0x0203 => restore_from_tray(hwnd),       // WM_LBUTTONDBLCLK
            _ => {}
        },
        win::WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u32;
            if id == ID_TRAY_OPEN {
                restore_from_tray(hwnd);
                return 0;
            }
            if id == ID_TRAY_EXIT {
                win::DestroyWindow(hwnd);
                return 0;
            }
        }
        win::WM_EXITSIZEMOVE => {
            save_geom(hwnd);
        }
        WM_GETMINMAXINFO if lparam != 0 => {
            let mmi = &mut *(lparam as *mut win::MINMAXINFO);
            mmi.ptMinTrackSize.x = WIN_MIN_W;
            mmi.ptMinTrackSize.y = WIN_MIN_H;
        }
        _ => {}
    }
    // 其余消息（含未拦截的 WM_COMMAND / WM_GETMINMAXINFO）交回原过程
    let prev = PREV_WND_PROC.load(Ordering::Relaxed);
    if prev != 0 {
        let prev_fn: win::WNDPROC = std::mem::transmute(prev);
        win::CallWindowProcW(prev_fn, hwnd, msg, wparam, lparam)
    } else {
        win::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

struct Options {
    dev: bool,
    url: Option<String>,
    dll: Option<PathBuf>,
    web: Option<PathBuf>,
    port: Option<u16>,
}

fn main() {
    log::init();
    let opts = parse_args();

    // ---- 1. 定位并加载 miniblink ----
    let dll = opts.dll.clone().unwrap_or_else(|| {
        find_dll().unwrap_or_else(|| {
            crate::logln!("找不到 mb132_x64.dll。请先运行 scripts/fetch-miniblink.sh 下载 SDK，");
            crate::logln!("或用 MB_DLL=<路径> / --dll <路径> 指定。");
            std::process::exit(2);
        })
    });
    crate::logln!("[mb] 加载内核: {}", dll.display());
    if let Err(e) = ffi::load(&dll) {
        crate::logln!("[mb] {e}");
        std::process::exit(3);
    }

    let mb = ffi::mb();
    // 注意：不要在这里调 mbEnableHighDPISupport()。
    // 它内部会触发 Chromium 的 AtExitManager 断言（RegisterCallback without an AtExitManager），
    // 因为此时 base 尚未初始化。官方 demo（demo_src/wkexe_linux.cpp）也是直接 mbInit(nullptr)。
    unsafe { (mb.mbInit)(std::ptr::null()) };
    crate::logln!("[mb] 初始化完成");

    // ---- 2. 起本地静态服务（喂 Vue 产物）----
    let web_root = opts
        .web
        .clone()
        .or_else(|| std::env::var("MB_WEB_ROOT").ok().map(PathBuf::from))
        .or_else(find_web_root)
        .unwrap_or_else(|| PathBuf::from("build_dist"));

    let local_port = match server::spawn(web_root.clone()) {
        Ok(p) => p,
        Err(e) => {
            crate::logln!("[http] 启动失败: {e}");
            std::process::exit(4);
        }
    };
    // web_root 在内嵌模式下**用不到**（资源由 server.rs 从 exe 内的资源表提供），
    // 它只是回退路径的选择结果。所以日志里要以资源源为准，别报这个路径。
    crate::logln!(
        "[http] 已启动 http://127.0.0.1:{}/  ({})",
        local_port,
        server::source_desc_global()
    );

    // ---- 3. 计算窗口尺寸：优先用上次记住的，否则按工作区（避开任务栏）算默认 ----
    let rect = load_geom().unwrap_or_else(default_rect);
    let win_w = rect.right - rect.left;
    let win_h = rect.bottom - rect.top;

    // ---- 4. 创建无边框窗口 ----
    let view = unsafe {
        (mb.mbCreateWebCustomWindow)(
            std::ptr::null_mut() as HWND,
            WS_POPUP | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME,
            WS_EX_APPWINDOW,
            rect.left,
            rect.top,
            win_w,
            win_h,
        )
    };
    if view == 0 {
        crate::logln!("[mb] 创建窗口失败");
        std::process::exit(5);
    }

    // 子类化窗口过程以限制最小尺寸 / 处理托盘与退出（必须在窗口所在线程立即做，避免竞态）。
    // 要在 mbRunMessageLoop 之前完成，此后 WndProc 才会收到消息。
    let hwnd = unsafe { (mb.mbGetHostHWND)(view) };
    if hwnd != std::ptr::null_mut() {
        WINDOW_HWND.store(hwnd, Ordering::Relaxed);
        let prev = unsafe {
            win::SetWindowLongPtrW(hwnd, win::GWL_WNDPROC, wnd_proc as *const () as isize)
        };
        PREV_WND_PROC.store(prev, Ordering::Relaxed);
        // 任务栏 / 标题栏 / Alt+Tab 图标（必须在窗口所在线程做）
        unsafe { set_window_icon(hwnd) };
        // 运行期常驻托盘图标（解决"右下角托盘也没有"）
        unsafe { add_tray(hwnd) };
    }

    let title = ffi::cstr("miniblink + Vue 3 + Rust");
    unsafe {
        (mb.mbSetWindowTitle)(view, title.as_ptr() as *const c_char);
        (mb.mbOnJsQuery)(view, ipc::on_js_query, std::ptr::null_mut());
        (mb.mbOnDocumentReady)(view, on_document_ready, std::ptr::null_mut());
        // 运行时数据（LocalStorage / cookie / 磁盘缓存）统一落到用户数据目录。
        // 默认 exe 同级；若装在 Program Files 等不可写位置，自动回退到 %LOCALAPPDATA%\mbvue。
        // 两者都由卸载器清理（见 scripts/make_installer.nsi）。
        setup_user_data(mb, view);
        // 无边框窗口：留出 6px 边缘用于拉伸，并转发给 miniblink 处理命中测试
        let k1 = ffi::cstr("ncHittestPaddingWidth");
        let v1 = ffi::cstr("6");
        (mb.mbSetDebugConfig)(view, k1.as_ptr() as *const c_char, v1.as_ptr() as *const c_char);
        (mb.mbSetNavigationToNewWindowEnable)(view, 0);
        (mb.mbSetCspCheckEnable)(view, 0);
        (mb.mbSetDragEnable)(view, 0);
        (mb.mbResize)(view, win_w, win_h);
        (mb.mbShowWindow)(view, 1);
    }

    ipc::init_runtime(view, local_port, dll.display().to_string(), web_root.display().to_string());

    // ---- 5. 加载页面 ----
    let target = opts.url.clone().unwrap_or_else(|| {
        if opts.dev {
            "http://127.0.0.1:5173/".to_string()
        } else {
            format!("http://127.0.0.1:{}/", local_port)
        }
    });
    crate::logln!("[mb] 打开 {}", target);
    let url = ffi::cstr(&target);
    unsafe { (mb.mbLoadURL)(view, url.as_ptr() as *const c_char) };

    // ---- 6. 进入消息循环 ----
    unsafe { (mb.mbRunMessageLoop)() };

    crate::logln!("[mb] 退出");
}

// ---------------------------------------------------------------- 托盘

unsafe fn load_tray_icon() -> win::HICON {
    if let Some(ico) = find_icon_file() {
        let wide: Vec<u16> = ico
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let h = win::LoadImageW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            win::IMAGE_ICON,
            16,
            16,
            win::LR_LOADFROMFILE,
        );
        if !h.is_null() {
            return h as win::HICON;
        }
    }
    std::ptr::null_mut()
}

unsafe fn add_tray(hwnd: HWND) {
    TRAY_HWND.store(hwnd, Ordering::Relaxed);
    let icon = load_tray_icon();
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    let tip: Vec<u16> = "mbvue".encode_utf16().chain(std::iter::once(0)).collect();
    for (i, c) in tip.iter().take(127).enumerate() {
        nid.szTip[i] = *c;
    }
    if Shell_NotifyIconW(NIM_ADD, &mut nid) != 0 {
        TRAY_ADDED.store(true, Ordering::Relaxed);
        crate::logln!("[tray] 托盘图标已添加");
    } else {
        crate::logln!("[tray] Shell_NotifyIconW ADD 失败");
    }
}

unsafe fn remove_tray() {
    if !TRAY_ADDED.load(Ordering::Relaxed) {
        return;
    }
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &mut nid);
    TRAY_ADDED.store(false, Ordering::Relaxed);
    crate::logln!("[tray] 托盘图标已移除");
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = win::CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let open: Vec<u16> = "打开窗口\0".encode_utf16().collect();
    let exit: Vec<u16> = "退出\0".encode_utf16().collect();
    win::AppendMenuW(menu, 0, ID_TRAY_OPEN as usize, open.as_ptr()); // MF_STRING = 0
    win::AppendMenuW(menu, 0, ID_TRAY_EXIT as usize, exit.as_ptr());
    let mut pt: POINT = std::mem::zeroed();
    win::GetCursorPos(&mut pt);
    // 标准做法：弹菜单前把窗口置前，弹完发一个 WM_NULL 让菜单正确收起
    win::SetForegroundWindow(hwnd);
    win::TrackPopupMenu(menu, 0x0002, pt.x, pt.y, 0, hwnd, std::ptr::null()); // TPM_RIGHTBUTTON
    win::PostMessageW(hwnd, win::WM_NULL, 0, 0);
    win::DestroyMenu(menu);
}

unsafe fn restore_from_tray(hwnd: HWND) {
    win::ShowWindow(hwnd, 9); // SW_RESTORE
    win::SetForegroundWindow(hwnd);
}

// ---------------------------------------------------------------- 窗口尺寸

/// 工作区（排除任务栏）矩形
fn work_area() -> RECT {
    let mut wa: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        win::SystemParametersInfoW(
            win::SPI_GETWORKAREA,
            0,
            &mut wa as *mut RECT as *mut std::ffi::c_void,
            0,
        )
    } != 0;
    if ok {
        wa
    } else {
        let w = unsafe { win::GetSystemMetrics(win::SM_CXSCREEN) };
        let h = unsafe { win::GetSystemMetrics(win::SM_CYSCREEN) };
        RECT { left: 0, top: 0, right: w, bottom: h }
    }
}

/// 默认窗口矩形：在工作区内居中，上下各留 16px，且不超过工作区。
fn default_rect() -> RECT {
    let wa = work_area();
    let margin = 16;
    let wa_w = wa.right - wa.left;
    let wa_h = wa.bottom - wa.top;
    let w = WIN_W.min(wa_w).max(WIN_MIN_W);
    let h = WIN_H.min(wa_h.saturating_sub(2 * margin)).max(WIN_MIN_H);
    let x = wa.left + (wa_w - w) / 2;
    let y = wa.top + (wa_h - h) / 2;
    RECT { left: x, top: y, right: x + w, bottom: y + h }
}

/// 记住窗口尺寸的文件（落在用户数据目录）
fn geom_path() -> PathBuf {
    user_data_dir().join("mbvue.window.json")
}

fn save_geom(hwnd: HWND) {
    unsafe {
        // 最大化 / 最小化态不记尺寸，留着上一次的正常尺寸
        if win::IsZoomed(hwnd) != 0 || win::IsIconic(hwnd) != 0 {
            return;
        }
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if win::GetWindowRect(hwnd, &mut r) == 0 {
            return;
        }
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        if w < WIN_MIN_W || h < WIN_MIN_H {
            return;
        }
        let data = json!({ "x": r.left, "y": r.top, "w": w, "h": h });
        if let Ok(s) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::write(geom_path(), s);
        }
    }
}

fn load_geom() -> Option<RECT> {
    let s = std::fs::read_to_string(geom_path()).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    let x = v.get("x")?.as_i64()? as i32;
    let y = v.get("y")?.as_i64()? as i32;
    let w = v.get("w")?.as_i64()? as i32;
    let h = v.get("h")?.as_i64()? as i32;
    if w < WIN_MIN_W || h < WIN_MIN_H {
        return None;
    }
    let mut r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    // 把可能越出工作区的矩形拉回（至少部分可见），避免上次在双屏/不同分辨率下记的尺寸跑到屏幕外
    let wa = work_area();
    if r.right <= wa.left || r.bottom <= wa.top || r.left >= wa.right || r.top >= wa.bottom {
        return None;
    }
    if r.left < wa.left {
        let d = wa.left - r.left;
        r.left += d;
        r.right += d;
    }
    if r.top < wa.top {
        let d = wa.top - r.top;
        r.top += d;
        r.bottom += d;
    }
    if r.right > wa.right {
        let d = r.right - wa.right;
        r.left -= d;
        r.right -= d;
    }
    if r.bottom > wa.bottom {
        let d = r.bottom - wa.bottom;
        r.top -= d;
        r.bottom -= d;
    }
    Some(r)
}

/// 启动兜底：若窗口仍超出工作区（被任务栏/屏幕边缘切掉），拉回。
/// 顺带修掉"刚打开时右上角关闭按钮瞬间错位"——那是早期尺寸未定导致的，
/// 现在默认尺寸按工作区算、首帧就正确，这里只做保险。
fn ensure_window_ok() {
    let hwnd = WINDOW_HWND.load(Ordering::Relaxed);
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if win::GetWindowRect(hwnd, &mut r) == 0 {
            return;
        }
        let wa = work_area();
        if r.bottom > wa.bottom || r.top < wa.top || r.right > wa.right || r.left < wa.left {
            let w = (r.right - r.left).clamp(WIN_MIN_W, (wa.right - wa.left).max(WIN_MIN_W));
            let h = (r.bottom - r.top).clamp(WIN_MIN_H, (wa.bottom - wa.top).max(WIN_MIN_H));
            let x = r.left.clamp(wa.left, (wa.right - wa.left - w).max(wa.left));
            let y = r.top.clamp(wa.top, (wa.bottom - wa.top - h).max(wa.top));
            win::SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                x,
                y,
                w,
                h,
                0x0004, // SWP_NOZORDER
            );
            crate::logln!("[mb] 窗口越界，已拉回工作区 {w}x{h} @ {x},{y}");
        }
    }
}

unsafe extern "system" fn on_document_ready(
    _view: ffi::MbWebView,
    _param: *mut std::ffi::c_void,
    _frame: ffi::MbWebFrameHandle,
) {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(350));
        ensure_window_ok();
        ipc::emit("ready", serde_json::json!({ "from": "rust" }));
    });
}

/// 运行时窗口图标：任务栏按钮、标题栏左上角、Alt+Tab 预览都读它。
///
/// 与 build.rs 里 embed-resource 嵌进 exe 的图标互补：
///   前者管「跑起来的窗口」，后者管「资源管理器里的 exe 文件」。
/// 两条路都通，任务栏才一定有图标（miniblink 的 mbCreateWebCustomWindow
/// 走的是自注册窗口类，不会自动带上 exe 资源里的图标）。
unsafe fn set_window_icon(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    let Some(ico) = find_icon_file() else {
        crate::logln!("[icon] 未找到 icon.ico，跳过窗口图标");
        return;
    };
    let wide: Vec<u16> = ico
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let big = win::LoadImageW(
        std::ptr::null_mut(),
        wide.as_ptr(),
        win::IMAGE_ICON,
        32,
        32,
        win::LR_LOADFROMFILE,
    );
    let small = win::LoadImageW(
        std::ptr::null_mut(),
        wide.as_ptr(),
        win::IMAGE_ICON,
        16,
        16,
        win::LR_LOADFROMFILE,
    );
    let mut ok = false;
    if !big.is_null() {
        win::SendMessageW(hwnd, win::WM_SETICON, win::ICON_BIG as usize, big as isize);
        ok = true;
    }
    if !small.is_null() {
        win::SendMessageW(hwnd, win::WM_SETICON, win::ICON_SMALL as usize, small as isize);
        ok = true;
    }
    if ok {
        crate::logln!("[icon] 窗口图标已设置: {}", ico.display());
    } else {
        crate::logln!("[icon] LoadImageW 失败: {}", ico.display());
    }
}

/// icon.ico 定位：优先 exe 同级目录（build.rs 已部署），
/// 再退回项目 assets/（从源码树直接跑时的兜底）。
fn find_icon_file() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("icon.ico");
            if p.exists() {
                return Some(p);
            }
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("icon.ico");
    p.exists().then_some(p)
}

fn parse_args() -> Options {
    let mut o = Options { dev: false, url: None, dll: None, web: None, port: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dev" => o.dev = true,
            "--url" => o.url = it.next(),
            "--dll" => o.dll = it.next().map(PathBuf::from),
            "--web" => o.web = it.next().map(PathBuf::from),
            "--port" => o.port = it.next().and_then(|v| v.parse().ok()),
            "-h" | "--help" => {
                crate::logln!(
                    "用法: mbvue [选项]\n\n\
                     --dev          连接 vite 开发服务器 (http://127.0.0.1:5173)\n\
                     --url <url>    直接加载指定 URL\n\
                     --dll <path>   指定 mb132_x64.dll 路径\n\
                     --web <dir>    指定前端产物目录（默认自动查找 build_dist）\n\
                     --port <port>  指定本地静态服务端口\n\n\
                     环境变量: MB_DLL / MB_WEB_ROOT / MB_PORT / MB_USER_DATA"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    if let Ok(p) = std::env::var("MB_PORT") {
        o.port = p.parse().ok();
    }
    o
}

/// 用户数据目录：默认 exe 同级；若那里不可写（如装在 Program Files 且未提权），
/// 回退到 `%LOCALAPPDATA%\mbvue`。
/// 可用 `MB_USER_DATA` 覆盖（便于验证脚本指向临时目录，不污染真实用户数据）。
/// 解析结果缓存到 `USER_DATA_DIR`，供 `setup_user_data` 与界面上报共用。
pub(crate) fn user_data_dir() -> PathBuf {
    USER_DATA_DIR.get().cloned().unwrap_or_else(resolve_user_data)
}

fn resolve_user_data() -> PathBuf {
    // 测试覆盖优先
    if let Ok(p) = std::env::var("MB_USER_DATA") {
        if !p.is_empty() {
            let dir = PathBuf::from(p);
            let _ = USER_DATA_DIR.set(dir.clone());
            return dir;
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            candidates.push(d.to_path_buf());
        }
    }
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(l).join("mbvue"));
    } else if let Ok(a) = std::env::var("APPDATA") {
        candidates.push(PathBuf::from(a).join("mbvue"));
    }
    for dir in &candidates {
        if std::fs::create_dir_all(dir).is_ok() {
            // 真正探测写权限：建个临时文件再删掉
            let probe = dir.join(".mbvue-writetest");
            if std::fs::write(&probe, b"").is_ok() {
                let _ = std::fs::remove_file(&probe);
                let _ = USER_DATA_DIR.set(dir.clone());
                return dir.clone();
            }
            // 目录在但不可写（Program Files 常见）→ 试下一个候选
            let _ = std::fs::remove_file(&probe);
        }
    }
    // 全不可写：退回 exe 同级（最差情况，写入会失败但路径确定，可由提权后重试）
    let fallback = candidates.into_iter().next().unwrap_or_else(|| PathBuf::from("."));
    let _ = USER_DATA_DIR.set(fallback.clone());
    crate::logln!("[store] 警告：所有候选目录都不可写，回退到 {}", fallback.display());
    fallback
}

/// 把内核的运行时数据落盘位置指到用户数据目录。
///
/// 背景：miniblink 默认把这些写在**exe 同级目录**：
///   · `LocalStorage\*.localsto`—— DOM storage（本项目用它持久化界面语种）
///   · `cookies.dat`          —— cookie jar
///   · 磁盘缓存目录
/// 默认就放 exe 同级（装到 Program Files 也这样，卸载器会一并清理，
/// 见 scripts/make_installer.nsi）；若 exe 同级不可写，则回退到 %LOCALAPPDATA%\mbvue
/// （同样由卸载器清理）。两种位置都不再像之前的版本那样"偷偷留在用户目录且不清理"。
///
/// 必须在 mbLoadURL **之前**调用：内核在首次导航时才真正打开这些存储。
unsafe fn setup_user_data(mb: &ffi::Mb, view: ffi::MbWebView) {
    let dir = user_data_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        crate::logln!("[store] 建用户数据目录失败 {}: {e}", dir.display());
        return;
    }

    // 这几个 API 收 WCHAR*（UTF-16），用 OsStr 转换以正确处理非 ASCII 路径
    let wide = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();

    let ls = wide(&dir);
    (mb.mbSetLocalStorageFullPath)(view, ls.as_ptr());

    let jar = wide(&dir.join("cookies.dat"));
    (mb.mbSetCookieJarFullPath)(view, jar.as_ptr());

    // 磁盘缓存：本地 HTTP 服务每次都带 Content-Length 且 Cache-Control: no-store，
    // 缓存没有价值，直接关掉省磁盘。
    (mb.mbSetDiskCacheEnabled)(view, 0);
    let cache = wide(&dir.join("cache"));
    (mb.mbSetDiskCachePath)(view, cache.as_ptr());

    crate::logln!("[store] 用户数据目录: {}", dir.display());
}

fn find_dll() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MB_DLL") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    // 安装包里用 core.dll（见 scripts/make_installer.sh），
    // 开发期 cargo 产物目录里还是 vendor 原名，两种都认。
    let names = ["core.dll", "mb132_x64.dll", "mb132_x32.dll"];
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
        }
    }
    if let Ok(t) = std::env::var("MB_TARGET_DIR") {
        dirs.push(PathBuf::from(t));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor").join("miniblink"));
    dirs.push(PathBuf::from("vendor/miniblink"));
    dirs.push(PathBuf::from("."));

    for d in dirs {
        for n in names {
            let p = d.join(n);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// 磁盘上前端产物目录（**仅开发期/未内嵌时用**）。
///
/// 优先级：exe 同级 → 项目内（cargo run）→ 当前工作目录。
/// 目录名沿用 vite 的 outDir（`build_dist`），同时认 `resources/` 作为备用，
/// 方便手工把产物放到别处测。
fn find_web_root() -> Option<PathBuf> {
    let dir_names = ["build_dist", "resources"];
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            for n in dir_names {
                cands.push(d.join(n));
            }
        }
    }
    for n in dir_names {
        cands.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(n));
        cands.push(PathBuf::from(n));
    }
    cands.into_iter().find(|p| Path::new(p).join("index.html").exists())
}
