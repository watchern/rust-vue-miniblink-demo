//! Windows 壳：miniblink（mb132）DLL + Win32 API。
//!
//! 从旧版单文件 main.rs 原样搬入，注释与踩坑记录全部保留。
//! 平台门面约定的导出面见 platform.rs 模块注释。

use std::os::raw::c_char;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, Ordering};
use std::sync::OnceLock;

use serde_json::{json, Value};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging as win;

use crate::ffi;
use crate::i18n;
use crate::server;
use crate::{Options, WIN_MIN_H, WIN_MIN_W};

const WS_POPUP: u32 = 0x8000_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
// 有它窗口才有 resize 边框：WS_POPUP 窗口默认不可拖拽缩放，
// 边缘命中测试由前端 ResizeHandle 组件负责。
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_EX_APPWINDOW: u32 = 0x0004_0000;

const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;

// ---- WM_NCHITTEST 命中区返回值（wnd_proc 返回 LRESULT = isize）----
const HTCLIENT: isize = 1;
const HTLEFT: isize = 10;
const HTRIGHT: isize = 11;
const HTBOTTOM: isize = 15;
const HTBOTTOMLEFT: isize = 16;
const HTBOTTOMRIGHT: isize = 17;
// ---- GetSystemMetrics 索引：无边框窗口的边框命中带宽 ----
const SM_CXSIZEFRAME: i32 = 32;
const SM_CYSIZEFRAME: i32 = 33;
const SM_CXPADDEDBORDER: i32 = 92;

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

/// WM_ERASEBKGND 兜底上色用的背景色是否用暗色（webview 铺满客户区后，
/// 只有启动瞬间/缩放过程中 webview 未及时重画时才会露出一角）。
/// 由前端在主题生效时经 app.setTheme 同步过来（启动时也先从 config.json 读一次）。
static FRAME_DARK: AtomicBool = AtomicBool::new(false);

/// 串行化对 mbRunJs / mbResponseQuery 的调用（原 ipc.rs 的 MB_LOCK，
/// emit 推送与 onJsQuery 回包都可能来自任意线程）
static MB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// WM_ERASEBKGND 兜底涂色的 COLORREF（0x00BBGGRR），跟随页面 --bg：
/// 暗色下 #14161b，浅色下 #f4f5f7。
fn frame_colors() -> u32 {
    if FRAME_DARK.load(Ordering::Relaxed) {
        0x001b_1614 // 暗：#14161b
    } else {
        0x00f7_f5f4 // 浅：#f4f5f7
    }
}

/// 子类化窗口过程：
///   · WM_NCCALCSIZE —— 客户区 = 整个窗口矩形（消灭 WS_THICKFRAME 的 7px
///     非客户区边框环；那圈不涂色，暗色下透出桌面/阴影/背后窗口显白边）
///   · WM_NCHITTEST —— 自算边框命中区：顶部整条给标题栏（JS 拖拽+窗口按钮），
///     左右下边框+底部两角给系统原生缩放
///   · WM_SETCURSOR —— 边框缩放区配系统缩放光标
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
        win::WM_ERASEBKGND => {
            // webview 铺满客户区，正常情况看不到这里的绘制；但启动瞬间/缩放
            // 过程中 webview 未及时重画时会闪一下类背景色，涂一层主题色兜底。
            let hdc = wparam as Gdi::HDC;
            let mut r: RECT = std::mem::zeroed();
            if !hdc.is_null() && win::GetClientRect(hwnd, &mut r) != 0 {
                let brush = Gdi::CreateSolidBrush(frame_colors());
                if !brush.is_null() {
                    Gdi::FillRect(hdc, &r, brush);
                    Gdi::DeleteObject(brush);
                }
            }
            return 1; // 非零 = 已擦背景，别让默认过程再用浅色类刷子涂
        }
        WM_NCCALCSIZE => {
            // 无边框可缩放窗口的标准做法（Electron/Chromium frame:false 同款）：
            // 客户区 = 整个窗口矩形。DefWindowProc 的默认行为会按 WS_THICKFRAME
            // 扣出一圈 ~7px 非客户区边框，而那圈系统不涂色——暗色主题下左侧透出
            // 桌面、下/右侧透出 DWM 阴影显浅灰、顶部透出背后的窗口显白，
            // 正是"暗色下顶部/底部/右侧一圈白边"的根源。扣掉后 webview 直接
            // 铺满整个窗口，四边颜色完全由页面绘制。边缘缩放命中测试改到
            // WM_NCHITTEST 里自己算，与客户区大小无关。
            //
            // 注意：光有本处理还不够——miniblink 的 webview 渲染视口会按窗口
            // **样式**算内边距。创建时若带 WS_THICKFRAME，视口从出生就内嵌
            // 7px（定位在 (7,7)），之后没有任何 API 改得回来；子类化后只补
            // WS_THICKFRAME（无 CAPTION）也一样。正解是「创建时不带
            // WS_THICKFRAME + 子类化后补 WS_THICKFRAME|WS_CAPTION|WS_SYSMENU」
            // （见 main 里创建窗口处的注释），视口从出生就铺满。
            if wparam != 0 {
                // wparam=TRUE 时 lparam 指向 NCCALCSIZE_PARAMS，rgrc[0] 是
                // 建议客户区（屏幕坐标）。最大化时系统会把窗口矩形撑大到工作
                // 区之外（好让默认客户区正好落在工作区内），必须把 rgrc[0]
                // 钳回工作区，否则客户区比可见区大一圈：UI 整体偏移、右上角
                // 按钮被屏幕边缘裁掉。
                let params = &mut *(lparam as *mut win::NCCALCSIZE_PARAMS);
                if win::IsZoomed(hwnd) != 0 {
                    let hmon = Gdi::MonitorFromWindow(hwnd, 2); // MONITOR_DEFAULTTONEAREST
                    if !hmon.is_null() {
                        let mut mi: Gdi::MONITORINFO = std::mem::zeroed();
                        mi.cbSize = std::mem::size_of::<Gdi::MONITORINFO>() as u32;
                        if Gdi::GetMonitorInfoW(hmon, &mut mi) != 0 {
                            params.rgrc[0] = mi.rcWork;
                        }
                    }
                }
            }
            return 0;
        }
        WM_NCHITTEST => {
            // 自己算边框命中区（实测验证过的完整链路）：
            //   · 顶部 yf 整带给 HTCLIENT——那一带整个是标题栏，JS 拖拽
            //     （win.drag 伪造 WM_NCLBUTTONDOWN+HTCAPTION）和右上角窗口
            //     按钮都靠 webview 收鼠标事件；若交给系统会变成 HTTOP 缩放
            //     区，顶部一条既拖不动也点不中按钮；
            //   · 左/右/下边框与底部两角给 HTLEFT/HTRIGHT/HTBOTTOM/
            //     HTBOTTOM*——随后的 WM_NCLBUTTONDOWN 经 miniblink 透传到
            //     DefWindowProc，原生 resize 模态循环启动（实测：命中→按下→
            //     窗口尺寸精确跟随鼠标，EVENT_SYSTEM_MOVESIZESTART/END 齐全）；
            //   · 其余 HTCLIENT，鼠标事件进 webview。
            //   yf 的两个分量都取 SM_CX* 版本：本机 GetSystemMetrics
            //   (SM_CYPADDEDBORDER=93) 返回垃圾值 648，纵方向用它会把命中带
            //   撑到 652px、整个窗口变成"顶部条"（踩过）。
            let mut pt = POINT {
                x: (lparam & 0xFFFF) as i16 as i32,
                y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
            };
            if Gdi::ScreenToClient(hwnd, &mut pt) != 0 {
                let mut cr: RECT = std::mem::zeroed();
                if win::GetClientRect(hwnd, &mut cr) != 0
                    && pt.x >= cr.left
                    && pt.x < cr.right
                    && pt.y >= cr.top
                    && pt.y < cr.bottom
                {
                    let xf = win::GetSystemMetrics(SM_CXSIZEFRAME)
                        + win::GetSystemMetrics(SM_CXPADDEDBORDER);
                    // 注意：纵方向也用 SM_CXPADDEDBORDER——本机
                    // GetSystemMetrics(SM_CYPADDEDBORDER=93) 返回垃圾值 648，
                    // 直接用会把命中带撑到 652px，整个窗口都变成"顶部条"。
                    let yf = win::GetSystemMetrics(SM_CYSIZEFRAME)
                        + win::GetSystemMetrics(SM_CXPADDEDBORDER);
                    let hit: isize = if pt.y < cr.top + yf {
                        HTCLIENT // 顶部整条：标题栏地盘
                    } else if pt.y >= cr.bottom - yf {
                        if pt.x < cr.left + xf {
                            HTBOTTOMLEFT
                        } else if pt.x >= cr.right - xf {
                            HTBOTTOMRIGHT
                        } else {
                            HTBOTTOM
                        }
                    } else if pt.x < cr.left + xf {
                        HTLEFT
                    } else if pt.x >= cr.right - xf {
                        HTRIGHT
                    } else {
                        HTCLIENT
                    };
                    return hit;
                }
            }
            // 视野外或异常：交回原过程
        }
        win::WM_SETCURSOR => {
            // lparam 低字是当前命中区。边框缩放区给系统缩放光标（原生观感），
            // 其余（HTCLIENT 等）交回原过程，webview 自己管光标。
            let hit = (lparam & 0xFFFF) as u16 as isize;
            let idi = match hit {
                HTLEFT | HTRIGHT => 32644,             // IDC_SIZEWE
                HTBOTTOM => 32645,                     // IDC_SIZENS
                HTBOTTOMLEFT | HTBOTTOMRIGHT => 32642, // IDC_SIZENWSE
                _ => 0,
            };
            if idi != 0 {
                let cur = win::LoadCursorW(std::ptr::null_mut(), idi as *const u16);
                if !cur.is_null() {
                    win::SetCursor(cur);
                }
                return 1; // TRUE：已处理，别再走默认类光标
            }
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

// ---------------------------------------------------------------- 主流程

/// Windows 壳主流程（原 main() 主体）：加载 miniblink → HTTP 服务 → 无边框窗口 →
/// 消息循环。阻塞到窗口关闭。
pub(crate) fn run(opts: Options) {
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
        .or_else(crate::find_web_root)
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

    // 启动即按 config.json 的主题给窗口那圈背景留白定色：暗色用户冷启时不露白边。
    // theme=system 这里先按浅色（前端 matchMedia 会立刻经 app.setTheme 纠正生效主题）。
    let cfg0 = crate::read_config();
    let saved_theme = cfg0.get("theme").and_then(|v| v.as_str()).unwrap_or("");
    FRAME_DARK.store(saved_theme == "dark", Ordering::Relaxed);

    // ---- 4. 创建无边框窗口 ----
    // 注意：创建时**故意不带 WS_THICKFRAME**。miniblink 的 webview 渲染视口
    // 按窗口样式算内边距：创建时若带 WS_THICKFRAME，DefWindowProc 的
    // WM_NCCALCSIZE 会扣出一圈 7px 边框，视口就按那个「带边框的客户区」
    // 初始化并缓存（定位在 (7,7)）——之后无论怎么 mbMoveWindow / 补发
    // WM_SIZE 都改不回来（实测：补发合成 WM_SIZE 被按旧尺寸跳过；渲染器
    // 进程就绪后还会再重置一次），四边各露一圈未涂色带，暗色主题下透出
    // 桌面/背后的窗口显白/灰边——正是用户报告的"暗色下顶部/底部/右侧白边"。
    // 无边框创建 → 创建时客户区就是整个窗口 → 视口从出生就是对的。
    // 边框样式在子类化后补（见下）：补齐后视口依然铺满（补 CAPTION 是关键，
    // 只补 THICKFRAME 无 CAPTION 时视口仍会内嵌 7px——实测四种组合得出）。
    let view = unsafe {
        (mb.mbCreateWebCustomWindow)(
            std::ptr::null_mut() as HWND,
            WS_POPUP | WS_MINIMIZEBOX | WS_MAXIMIZEBOX,
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
        // 子类化后补上 WS_THICKFRAME | WS_CAPTION | WS_SYSMENU（Electron/Chromium
        // frame:false 同款组合）：
        //   · WS_THICKFRAME —— 系统原生 resize 边框命中（WM_NCHITTEST 自算命中区
        //     走它）、最大化窗口矩形撑大；
        //   · WS_CAPTION —— DWM 阴影、最大化动画、Win+方向键贴靠都要求样式里有
        //     标题栏位；标题栏本身由 WM_NCCALCSIZE 划进客户区，不可见；
        //   · WS_SYSMENU —— Alt+空格 系统菜单等标准行为。
        // 实测：只补 WS_THICKFRAME（无 CAPTION）时 miniblink 的渲染视口会按
        // 「带边框客户区」内嵌 ~7px，四边各露一圈未涂色带（暗色下显白/灰边，
        // 即用户报告的"暗色主题顶部/底部/右侧白边"）；补上 CAPTION 后视口铺满。
        // 补样式位会让系统重算一次 NCCALCSIZE——我们的处理恒返回「客户区 =
        // 整个窗口」，所以客户区不变，但系统/DWM 会按新样式重新评估阴影与边框。
        unsafe {
            let style = win::GetWindowLongPtrW(hwnd, win::GWL_STYLE) as u32;
            win::SetWindowLongPtrW(hwnd, win::GWL_STYLE, (style | WS_THICKFRAME | WS_CAPTION | WS_SYSMENU) as isize);
            win::SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020, // SWP_NOSIZE|SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_FRAMECHANGED
            );
        }
        // 任务栏 / 标题栏 / Alt+Tab 图标（必须在窗口所在线程做）
        unsafe { set_window_icon(hwnd) };
        // 运行期常驻托盘图标（解决"右下角托盘也没有"）
        unsafe { add_tray(hwnd) };
    }

    let title = ffi::cstr("miniblink + Vue 3 + Rust");
    unsafe {
        (mb.mbSetWindowTitle)(view, title.as_ptr() as *const c_char);
        (mb.mbOnJsQuery)(view, on_js_query, std::ptr::null_mut());
        (mb.mbOnDocumentReady)(view, on_document_ready, std::ptr::null_mut());
        // 运行时数据（LocalStorage / cookie / 磁盘缓存）统一落到用户数据目录。
        // 默认 exe 同级；若装在 Program Files 等不可写位置，自动回退到 %LOCALAPPDATA%\mbvue。
        // 两者都由卸载器清理（见 scripts/make_installer.nsi）。
        setup_user_data(mb, view);
        // 不设 ncHittestPaddingWidth，也不留非客户区边框环（WM_NCCALCSIZE 已把
        // 客户区扩到整个窗口）：那圈留白会被 miniblink 内部涂成固定色（顶部白、
        // 左侧黑），暗色主题下露一圈白边——实测无解。现在 webview 铺满整个窗口，
        // 四边颜色完全由页面绘制。缩放由 WM_NCHITTEST 自算的边框命中区（左右下 +
        // 底部两角，系统原生）+ 前端 ResizeHandle 把手共同完成。
        (mb.mbSetNavigationToNewWindowEnable)(view, 0);
        (mb.mbSetCspCheckEnable)(view, 0);
        (mb.mbSetDragEnable)(view, 0);
        // 不设 ncHittestPaddingWidth，也不留非客户区边框环（WM_NCCALCSIZE 已把
        // 客户区扩到整个窗口）：那圈留白会被 miniblink 内部涂成固定色（顶部白、
        // 左侧黑），暗色主题下露一圈白边——实测无解。现在 webview 铺满整个窗口，
        // 四边颜色完全由页面绘制。缩放由 WM_NCHITTEST 自算的边框命中区（左右下 +
        // 底部两角，系统原生）+ 前端 ResizeHandle 把手共同完成。
        //
        // 注意：**不要**在启动时调 mbMoveWindow(view, 0, 0, ...)。它的 x/y 是
        // 屏幕坐标，会把宿主窗口从居中位置挪到屏幕 (0,0)（实测踩过）；
        // 而无边框创建时客户区本就是整个窗口，webview 视口从出生就是对的，
        // 不需要任何启动期布局调用。
        (mb.mbShowWindow)(view, 1);
    }

    crate::ipc::init_runtime(view, local_port, dll.display().to_string(), web_root.display().to_string());

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
    let w = 1180.min(wa_w).max(WIN_MIN_W);
    let h = 760.min(wa_h.saturating_sub(2 * margin)).max(WIN_MIN_H);
    let x = wa.left + (wa_w - w) / 2;
    let y = wa.top + (wa_h - h) / 2;
    RECT { left: x, top: y, right: x + w, bottom: y + h }
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
        // 读-改-写：只更新 `window` 段，保留 config.json 里其它配置项（便于后期扩展）
        crate::write_window_section(r.left, r.top, w, h);
    }
}

fn load_geom() -> Option<RECT> {
    // 优先 config.json（window 段）；兼容旧版 mbvue.window.json（扁平 x/y/w/h）
    let legacy_path = user_data_dir().join("mbvue.window.json");
    let (s, legacy) = match std::fs::read_to_string(crate::config_path()) {
        Ok(s) => (s, false),
        Err(_) => match std::fs::read_to_string(&legacy_path) {
            Ok(s) => (s, true),
            Err(_) => return None,
        },
    };
    let v: Value = serde_json::from_str(&s).ok()?;
    let win_node = if legacy { v.clone() } else { v.get("window")?.clone() };
    let x = win_node.get("x")?.as_i64()? as i32;
    let y = win_node.get("y")?.as_i64()? as i32;
    let w = win_node.get("w")?.as_i64()? as i32;
    let h = win_node.get("h")?.as_i64()? as i32;
    // 旧版文件迁移到 config.json，并删除旧文件
    if legacy {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        root["window"] = json!({ "x": x, "y": y, "w": w, "h": h });
        if let Ok(out) = serde_json::to_string_pretty(&root) {
            let _ = std::fs::write(crate::config_path(), out);
            let _ = std::fs::remove_file(&legacy_path);
        }
    }
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
        // 兜底：若窗口仍超出工作区（被任务栏/屏幕边缘切掉），拉回。
        // 现在默认尺寸按工作区算、首帧就正确，这里只做保险。
        ensure_window_ok();
        crate::ipc::emit("ready", serde_json::json!({ "from": "rust" }));
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

/// 日志目录：Windows 维持旧行为——项目内 logs/（编译期确定，不随 exe 拷贝位置变化）。
pub(crate) fn log_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/logs"))
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

    // LocalStorage 放进 LocalStorage\ 子目录（miniblink 默认就是这么放的），
    // 不要把 *.localsto 散落在 exe 同级根目录。cookie / 磁盘缓存仍是单文件/单目录，留在根。
    let ls_dir = dir.join("LocalStorage");
    let _ = std::fs::create_dir_all(&ls_dir);
    let ls = wide(&ls_dir);
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

// ---------------------------------------------------------------- 平台门面实现

/// 把 JS 片段推给 webview：`window.__mbEmit(...)` / `window.__mbResolve(...)`
/// 都走这条（原 ipc.rs emit 的 mbRunJs 段）。fire-and-forget。
pub(crate) fn eval_js(js: String) {
    let view = crate::ipc::current_view();
    if view == 0 {
        return;
    }
    let c = ffi::cstr(&js);

    let _guard = MB_LOCK.lock();
    let mb = ffi::mb();
    unsafe {
        let frame = (mb.mbWebFrameGetMainFrame)(view);
        (mb.mbRunJs)(
            view,
            frame,
            c.as_ptr() as *const c_char,
            1,
            noop_runjs_cb,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
}

unsafe extern "system" fn noop_runjs_cb(
    _view: ffi::MbWebView,
    _param: *mut std::ffi::c_void,
    _es: ffi::MbJsExecState,
    _v: ffi::MbJsValue,
) {
}

/// onJsQuery 的 C 回调：解析请求 → 分发 → 回包（原 ipc.rs 的 on_js_query，
/// 只有 miniblink 路径需要它；Linux 走 UserContentManager 信号）。
pub unsafe extern "system" fn on_js_query(
    view: ffi::MbWebView,
    _param: *mut std::ffi::c_void,
    _es: ffi::MbJsExecState,
    query_id: i64,
    custom_msg: i32,
    request: *const c_char,
) {
    if request.is_null() {
        return;
    }
    let raw = std::ffi::CStr::from_ptr(request).to_string_lossy().to_string();
    if std::env::var("MB_TRACE").is_ok() {
        crate::logln!("[ipc] <- {}", &raw[..raw.len().min(160)]);
    }

    let response = match serde_json::from_str::<Value>(&raw) {
        Ok(v) => {
            let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("").to_string();
            let payload = v.get("payload").cloned().unwrap_or(Value::Null);
            crate::ipc::dispatch(&cmd, &payload)
        }
        Err(e) => json!({ "ok": false, "error": i18n::json_parse_failed(&e.to_string()) }),
    };

    let out = response.to_string();
    if std::env::var("MB_TRACE").is_ok() {
        crate::logln!("[ipc] -> {}", &out[..out.len().min(200)]);
    }
    let c = ffi::cstr(&out);
    let mb = ffi::mb();
    let _guard = MB_LOCK.lock();
    (mb.mbResponseQuery)(view, query_id, custom_msg, c.as_ptr() as *const c_char);
}

/// 主题同步：兜底背景色跟随（暗色不露白边）。
pub(crate) fn set_theme(dark: bool) {
    FRAME_DARK.store(dark, Ordering::Relaxed);
    // 主题变了但窗口没 resize，兜底背景不会被自动重擦 → 主动失效+擦除。
    // 只失效父窗口：webview 不受影响、不会重绘，故不闪。
    let _ = with_hwnd(|h| unsafe {
        Gdi::RedrawWindow(
            h,
            std::ptr::null(),
            std::ptr::null_mut(),
            0x0001 | 0x0002 | 0x0100, // RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW
        );
    });
}

fn with_hwnd<T>(f: impl FnOnce(HWND) -> T) -> Result<T, String> {
    let view = crate::ipc::current_view();
    if view == 0 {
        return Err(i18n::webview_not_ready());
    }
    let h = unsafe { (ffi::mb().mbGetHostHWND)(view) };
    if h.is_null() {
        return Err(i18n::no_hwnd());
    }
    Ok(f(h))
}

fn win_cmd<R>(f: impl FnOnce(HWND) -> R, name: &str) -> Result<(), String> {
    with_hwnd(f)?;
    crate::logln!("[win] {}", name);
    Ok(())
}

pub(crate) fn win_minimize() -> Result<(), String> {
    // 收进托盘（隐藏窗口），托盘图标是恢复/退出的入口
    win_cmd(|h| unsafe { win::ShowWindow(h, 0) }, "minimize")
}

/// 已最大化则还原，否则最大化；返回新的最大化状态
pub(crate) fn win_maximize() -> Result<bool, String> {
    let zoomed = with_hwnd(|h| unsafe { win::IsZoomed(h) })? != 0;
    let code = if zoomed { 9 } else { 3 }; // SW_RESTORE / SW_MAXIMIZE
    win_cmd(|h| unsafe { win::ShowWindow(h, code) }, "maximize")?;
    Ok(!zoomed)
}

pub(crate) fn win_close() -> Result<(), String> {
    win_cmd(|h| unsafe { win::DestroyWindow(h) }, "close")
}

/// 无边框窗口拖拽：伪造一次标题栏按下
pub(crate) fn win_drag() -> Result<(), String> {
    win_cmd(
        |h| unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            win::SendMessageW(h, 0x00A1, 2, 0); // WM_NCLBUTTONDOWN + HTCAPTION
        },
        "drag",
    )
}

/// 前端 resize handle 拖拽时读基准尺寸（miniblink 的 WndProc
/// 不实现 WM_NCHITTEST 边缘缩放路径，只能前端自己算目标尺寸）
pub(crate) fn win_get_size() -> Result<(i32, i32), String> {
    let size = with_hwnd(|hwnd| unsafe {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if win::GetWindowRect(hwnd, &mut r) == 0 {
            (0, 0)
        } else {
            (r.right - r.left, r.bottom - r.top)
        }
    })?;
    Ok(size)
}

pub(crate) fn win_resize(w: i32, h: i32) -> Result<(), String> {
    win_cmd(
        |hwnd| unsafe {
            win::SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                w,
                h,
                0x0002 | 0x0004, // SWP_NOMOVE | SWP_NOZORDER
            );
        },
        "resize",
    )
}

pub(crate) fn win_set_title(t: &str) -> Result<(), String> {
    let view = crate::ipc::current_view();
    if view == 0 {
        return Err(i18n::webview_not_ready());
    }
    let c = ffi::cstr(t);
    unsafe { (ffi::mb().mbSetWindowTitle)(view, c.as_ptr() as *const c_char) };
    Ok(())
}
