//! Linux 壳：GTK3 + WebKitGTK 4.1（libsoup3）。
//!
//! 为什么不用 miniblink：官方 release 只发 Windows DLL
//! （vendor/miniblink/demo_src/wkexe_linux.cpp 只是源码级 demo，
//! 从源码编 Chromium 内核对发布不可行）。WebKitGTK 是 Linux 桌面
//! WebShell 的成熟路线（Tauri/wry 同款）。
//!
//! 与 Windows 端（platform_win.rs）的能力对照：
//!   · 无边框窗口     gtk_window_set_decorated(false)，标题栏仍由前端绘制
//!   · 标题栏拖拽     win.drag → gtk_window_begin_move_drag（X11 可靠；
//!                    Wayland 要求真实指针事件发起，能否接受由合成器决定，
//!                    README 已记录此差异）
//!   · 缩放           无边框窗口没有系统边缘命中带：靠前端 ResizeHandle 把手
//!                    → win.resize → gtk_window_resize
//!   · 托盘           无对应实现（需要额外的 libappindicator 依赖），
//!                    win.minimize 语义 = 最小化到任务栏
//!   · JS 桥          UserContentManager 的 script message handler 复刻
//!                    window.mbQuery；出站事件用 run_javascript 调
//!                    window.__mbEmit——前端 bridge.ts 零改动
//!   · 主题同步       窗口背景即页面背景，无 WM_ERASEBKGND 等价物，no-op
//!   · 尺寸记忆       复用 config.json 的 window 段（与 Windows 共用格式）
//!
//! 线程模型：所有 GTK/WebKit 调用必须在主线程。后台线程（tick 线程等）
//! 经 glib MainContext channel 把 JS 片段派发回主线程执行。
//!
//! API 签名核对基线：webkit2gtk 2.0.2 / gtk 0.18.2 / javascriptcore-rs 1.1.2
//! （本地 registry 源码逐一核对过，见 git 历史说明）。

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use gtk::prelude::*;
use gtk::glib;

use serde_json::Value;
use webkit2gtk::{
    JavascriptResult, LoadEvent, UserContentInjectedFrames, UserScript, UserScriptInjectionTime,
    WebView,
};

use crate::logln;
use crate::{Options, WIN_MIN_H, WIN_MIN_W};

// glib/gtk 对象不是 Send/Sync（glib-rs 0.18 只有 WeakRef/Sender 等包装可跨线程），
// 所以窗口用 thread_local 存放——所有 win_* 命令都由 ipc dispatch 触发，
// 而 Linux 端 dispatch 只发生在 script-message-handler 回调（GTK 主线程）。
// 若将来有后台线程误调用，thread_local 会在那条线程上取到 None，安全降级为
// "webview 未就绪"错误，不会崩。
thread_local! {
    static WINDOW: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
}
/// eval_js 的派发通道：任意线程 send，主循环任务 recv 后 run_javascript。
/// glib Sender 是 Send 但不保证 Sync，用 Mutex 包一层（emit 频率低，开销可忽略）。
static JS_TX: OnceLock<Mutex<Option<glib::Sender<String>>>> = OnceLock::new();
/// 用户数据目录（解析一次后缓存）
static USER_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
/// configure-event 节流：上次写 config.json 的时刻（毫秒）
static LAST_GEOM_SAVE_MS: AtomicU64 = AtomicU64::new(0);

/// document-start 注入的桥接 shim。
///
/// 与 miniblink 注入的 window.mbQuery(customMsg, request, cb) 保持同一签名：
/// 内部把请求经 webkit.messageHandlers.mbvue 送到 Rust，Rust 处理完回放
/// __mbResolve(id, response) 触发 cb。cb(customMsg, response) 的 customMsg
/// 固定回 0（Windows 端 miniblink 也是这样回的），bridge.ts 只读 response。
const SHIM_JS: &str = r#"(function () {
  if (window.mbQuery) return;
  var seq = 0;
  var cbs = {};
  window.mbQuery = function (customMsg, request, cb) {
    var id = ++seq;
    cbs[id] = cb || function () {};
    try {
      window.webkit.messageHandlers.mbvue.postMessage(
        JSON.stringify({ id: id, msg: customMsg, request: request })
      );
    } catch (e) {
      var cbErr = cbs[id];
      delete cbs[id];
      if (cbErr) cbErr(0, JSON.stringify({ ok: false, error: 'bridge: ' + e }));
    }
  };
  window.__mbResolve = function (id, response) {
    var cb = cbs[id];
    if (!cb) return;
    delete cbs[id];
    try { cb(0, response); } catch (e) { console.error('[mbvue] resolve failed', e); }
  };
})();"#;

// ---------------------------------------------------------------- 主流程

/// Linux 壳主流程：HTTP 服务 → 无边框 GTK 窗口 + WebKitGTK → 消息循环。
/// 阻塞到窗口关闭。
pub(crate) fn run(opts: Options) {
    gtk::init().unwrap_or_else(|e| {
        logln!("[gtk] 初始化失败: {e}");
        std::process::exit(3);
    });

    // ---- 1. 起本地静态服务（喂 Vue 产物，与 Windows 同款）----
    let web_root = opts
        .web
        .clone()
        .or_else(|| std::env::var("MB_WEB_ROOT").ok().map(PathBuf::from))
        .or_else(crate::find_web_root)
        .unwrap_or_else(|| PathBuf::from("build_dist"));
    let local_port = match crate::server::spawn(web_root.clone()) {
        Ok(p) => p,
        Err(e) => {
            logln!("[http] 启动失败: {e}");
            std::process::exit(4);
        }
    };
    logln!(
        "[http] 已启动 http://127.0.0.1:{}/  ({})",
        local_port,
        crate::server::source_desc_global()
    );

    // ---- 2. 无边框窗口 ----
    // WindowType::Popup 是 override-redirect 窗口（无任务栏入口、不参与焦点），
    // 必须用 Toplevel + set_decorated(false)（Electron frame:false 同款语义）。
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("miniblink + Vue 3 + Rust");
    win.set_decorated(false);
    win.set_default_size(1180, 760);
    // 最小尺寸：等价 Windows 端 WM_GETMINMAXINFO 的 ptMinTrackSize
    win.set_size_request(WIN_MIN_W, WIN_MIN_H);
    if let Some(icon) = find_icon_file() {
        if let Err(e) = win.set_icon_from_file(&icon) {
            logln!("[icon] 设置窗口图标失败: {e}");
        }
    }

    // 恢复上次几何（config.json 的 window 段，与 Windows 共用格式）
    if let Some((x, y, w, h)) = load_geom() {
        win.move_(x, y);
        win.resize(w, h);
    }

    // ---- 3. WebKitGTK + JS 桥 ----
    let ucm = webkit2gtk::UserContentManager::new();
    // document-start 注入 shim：window.mbQuery 必须在页面脚本运行前就位
    //（bridge.ts 的 invoke 在挂载后才会调用，但防御性提前注入）
    ucm.add_script(&UserScript::new(
        SHIM_JS,
        UserContentInjectedFrames::TopFrame,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    ));
    if !ucm.register_script_message_handler("mbvue") {
        logln!("[ipc] register_script_message_handler 返回 false（消息桥可能不可用）");
    }
    ucm.connect_script_message_received(Some("mbvue"), |_, result| {
        on_script_message(result);
    });

    let webview = WebView::with_user_content_manager(&ucm);
    win.add(&webview);

    // eval 派发通道：后台线程 emit() → 这里 recv → 主线程 run_javascript。
    // 回包（__mbResolve）也走这条，与事件推送保持同一队列顺序。
    // 持有 WebView 的 future 不是 Send，必须 spawn_local（挂主线程默认上下文）。
    let (tx, rx) = glib::MainContext::channel::<String>(glib::PRIORITY_DEFAULT);
    let _ = JS_TX.set(Mutex::new(Some(tx)));
    {
        let webview = webview.clone();
        glib::MainContext::default().spawn_local(async move {
            while let Ok(js) = rx.recv().await {
                // fire-and-forget：与 Windows 端 mbRunJs 的语义一致
                webview.run_javascript(&js, Option::<&gtk::gio::Cancellable>::None, |_| {});
            }
        });
    }

    // 页面加载完成 → ready 事件（对齐 Windows on_document_ready 的 350ms 延迟）
    webview.connect_load_changed(move |_, ev| {
        if ev == LoadEvent::Finished {
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), || {
                crate::ipc::emit("ready", serde_json::json!({ "from": "rust" }));
            });
        }
    });

    // ---- 4. 几何记忆与退出 ----
    // configure-event 在拖拽/缩放过程中高频触发，节流 500ms 落盘
    //（Windows 端只在 WM_EXITSIZEMOVE 落一次盘，这里以节流近似）
    win.connect_configure_event(|win, _ev| {
        let now = now_ms();
        let last = LAST_GEOM_SAVE_MS.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= 500 {
            LAST_GEOM_SAVE_MS.store(now, Ordering::Relaxed);
            save_geom(win);
        }
        true // 继续传播，不影响默认布局处理
    });
    win.connect_delete_event(|win, _ev| {
        save_geom(win);
        gtk::main_quit();
        // 拦下默认的销毁动作：main_quit 结束循环后由我们统一退出
        glib::Propagation::Stop
    });

    crate::ipc::init_runtime(
        0,
        local_port,
        "WebKitGTK (system library)".to_string(),
        web_root.display().to_string(),
    );

    // ---- 5. 加载页面 ----
    let target = opts.url.clone().unwrap_or_else(|| {
        if opts.dev {
            "http://127.0.0.1:5173/".to_string()
        } else {
            format!("http://127.0.0.1:{}/", local_port)
        }
    });
    logln!("[mb] 打开 {}", target);
    webview.load_uri(&target);

    win.show_all();
    WINDOW.with(|w| *w.borrow_mut() = Some(win.clone()));

    // ---- 6. 进入主循环 ----
    gtk::main();
}

// ---------------------------------------------------------------- JS 桥

/// script-message-handler 入口：取消息文本 → 分发 → 排队回包。
/// 队列由主循环任务消费（见 run()），此刻就在主线程，走通道保证顺序。
fn on_script_message(result: &JavascriptResult) {
    use javascriptcore::ValueExt as _;

    let Some(v) = result.js_value() else {
        logln!("[ipc] 收到空消息，忽略");
        return;
    };
    if !v.is_string() {
        logln!("[ipc] 收到非字符串消息，忽略");
        return;
    }
    let raw = v.to_str().to_string();
    if std::env::var("MB_TRACE").is_ok() {
        logln!("[ipc] <- {}", &raw[..raw.len().min(160)]);
    }

    // envelope: {id, msg, request}；request 是 bridge.ts 的 {cmd, payload} JSON 字符串
    let response = match serde_json::from_str::<Value>(&raw) {
        Ok(env) => {
            let id = env.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let request = env.get("request").and_then(|v| v.as_str()).unwrap_or("");
            let resp = match serde_json::from_str::<Value>(request) {
                Ok(req) => {
                    let cmd = req.get("cmd").and_then(|c| c.as_str()).unwrap_or("").to_string();
                    let payload = req.get("payload").cloned().unwrap_or(Value::Null);
                    crate::ipc::dispatch(&cmd, &payload)
                }
                Err(e) => serde_json::json!({
                    "ok": false,
                    "error": crate::i18n::json_parse_failed(&e.to_string())
                }),
            };
            format!("window.__mbResolve({},{});", id, resp)
        }
        Err(e) => {
            logln!("[ipc] envelope 解析失败: {e}");
            return;
        }
    };

    if std::env::var("MB_TRACE").is_ok() {
        logln!("[ipc] -> {}", &response[..response.len().min(200)]);
    }
    eval_js(response);
}

// ---------------------------------------------------------------- 几何记忆

fn save_geom(win: &gtk::Window) {
    // 最大化态不记尺寸，留着上一次的正常尺寸
    if win.is_maximized() {
        return;
    }
    let (x, y) = win.position();
    let alloc = win.allocation();
    let (w, h) = (alloc.width(), alloc.height());
    if w < WIN_MIN_W || h < WIN_MIN_H {
        return;
    }
    crate::write_window_section(x, y, w, h);
}

fn load_geom() -> Option<(i32, i32, i32, i32)> {
    let s = std::fs::read_to_string(crate::config_path()).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    let win = v.get("window")?;
    let x = win.get("x")?.as_i64()? as i32;
    let y = win.get("y")?.as_i64()? as i32;
    let w = win.get("w")?.as_i64()? as i32;
    let h = win.get("h")?.as_i64()? as i32;
    if w < WIN_MIN_W || h < WIN_MIN_H {
        return None;
    }
    // 越出屏幕的几何交给窗口管理器钳制（GTK/compositor 会拉回可见区）
    Some((x, y, w, h))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- 资源定位

/// icon.png 定位：exe 同级（便携包布局）→ 安装位（deb/rpm/AppImage）→ 项目 assets/（开发期）。
fn find_icon_file() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // 便携 tar.gz / AppImage：icon 与可执行文件同级
            cands.push(dir.join("icon.png"));
            // 安装布局的图标在 ../icons/...（exe 在 /usr/bin 时无妨，先兜底用安装位路径）
            cands.push(PathBuf::from("/usr/share/icons/hicolor/256x256/apps/mbvue.png"));
        }
    }
    cands.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("icon.png"));
    cands.into_iter().find(|p| p.exists())
}

// ---------------------------------------------------------------- 平台门面实现

/// 用户数据目录：`$MB_USER_DATA` → `$XDG_DATA_HOME/mbvue`（默认 ~/.local/share/mbvue）。
/// deb/rpm 装到 /usr/bin 后那里不可写，XDG 路径是标准落点。
pub(crate) fn user_data_dir() -> PathBuf {
    USER_DATA_DIR.get().cloned().unwrap_or_else(resolve_user_data)
}

fn resolve_user_data() -> PathBuf {
    // 测试覆盖优先（与 Windows 端一致）
    if let Ok(p) = std::env::var("MB_USER_DATA") {
        if !p.is_empty() {
            let dir = PathBuf::from(p);
            let _ = USER_DATA_DIR.set(dir.clone());
            return dir;
        }
    }
    // XDG：相对路径按规范忽略
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")));
    let dir = base
        .unwrap_or_else(|| PathBuf::from("/tmp/mbvue-data"))
        .join("mbvue");
    let _ = std::fs::create_dir_all(&dir);
    let _ = USER_DATA_DIR.set(dir.clone());
    dir
}

/// 日志目录：Linux 写用户数据目录（/usr/bin 下不可写，编译期路径也不存在）。
pub(crate) fn log_dir() -> PathBuf {
    user_data_dir()
}

/// 把 JS 片段排进主线程执行队列（fire-and-forget）。
pub(crate) fn eval_js(js: String) {
    if let Some(m) = JS_TX.get() {
        if let Ok(g) = m.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(js);
            }
        }
    }
}

/// 主题同步：窗口背景即页面背景，无需兜底涂色（Windows 端的 WM_ERASEBKGND 补色
/// 在 GTK 上没有对应物）。
pub(crate) fn set_theme(_dark: bool) {}

/// 取窗口句柄（见顶部 statics 注释：thread_local + clone 出引用计数句柄）。
fn window() -> Result<gtk::Window, String> {
    WINDOW
        .with(|w| w.borrow().clone())
        .ok_or_else(|| crate::i18n::webview_not_ready())
}

/// Windows：收进托盘；Linux 无托盘实现 → 最小化到任务栏
pub(crate) fn win_minimize() -> Result<(), String> {
    window()?.iconify();
    Ok(())
}

/// 已最大化则还原，否则最大化；返回新的最大化状态
pub(crate) fn win_maximize() -> Result<bool, String> {
    let win = window()?;
    let zoomed = win.is_maximized();
    if zoomed {
        win.unmaximize();
    } else {
        win.maximize();
    }
    Ok(!zoomed)
}

pub(crate) fn win_close() -> Result<(), String> {
    let win = window()?;
    save_geom(win);
    gtk::main_quit();
    Ok(())
}

/// 无边框窗口拖拽：从当前指针位置发起系统级移动循环。
/// 注意：发起时机不是真实 GDK 按键事件（script-message 回调），
/// X11 下按 CurrentTime 处理没问题；Wayland 由合成器裁决（README 已记录）。
pub(crate) fn win_drag() -> Result<(), String> {
    let win = window()?;
    let (px, py) = win
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer())
        .map(|dev| {
            let (_, x, y) = dev.position();
            (x, y)
        })
        .unwrap_or((0, 0));
    win.begin_move_drag(1, px, py, gtk::current_event_time());
    Ok(())
}

pub(crate) fn win_get_size() -> Result<(i32, i32), String> {
    let win = window()?;
    let alloc = win.allocation();
    Ok((alloc.width(), alloc.height()))
}

pub(crate) fn win_resize(w: i32, h: i32) -> Result<(), String> {
    window()?.resize(w, h);
    Ok(())
}

pub(crate) fn win_set_title(t: &str) -> Result<(), String> {
    window()?.set_title(t);
    Ok(())
}
