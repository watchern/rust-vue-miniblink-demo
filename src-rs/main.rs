// 通用入口：参数解析、日志、配置读写，然后分发到平台壳实现。
//
// Windows：GUI 子系统（双击 exe 不弹控制台），miniblink + Win32 壳，见 platform_win.rs
// Linux：  控制台程序，GTK3 + WebKitGTK 4.1 壳，见 platform_linux.rs
//
// 日志：Windows 固定写 <CARGO_MANIFEST_DIR>/logs/mbvue.log（编译期确定）；
//       Linux 写 <user_data_dir>/mbvue.log（装到 /usr/bin 后编译期路径不可写）。
#![cfg_attr(windows, windows_subsystem = "windows")]

mod i18n;
mod ipc;
mod log;
mod platform;
mod server;

#[cfg(windows)]
mod ffi;
#[cfg(windows)]
mod platform_win;
#[cfg(all(unix, not(target_os = "macos")))]
mod platform_linux;

pub(crate) use log::logln;

use std::path::PathBuf;
use serde_json::{json, Value};

// 最小尺寸：Windows 端 WM_GETMINMAXINFO 原生钳制；Linux 端 set_size_request；
// ipc.rs 的 win.resize 钳制也复用这两个常量
pub(crate) const WIN_MIN_W: i32 = 900;
pub(crate) const WIN_MIN_H: i32 = 600;

/// 界面上报的内核描述（app.info.engine）。两个平台各有一个内核实现。
#[cfg(windows)]
pub(crate) const ENGINE_NAME: &str = "miniblink 132 (Chromium 132)";
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const ENGINE_NAME: &str = "WebKitGTK 4.1";

pub(crate) struct Options {
    pub(crate) dev: bool,
    pub(crate) url: Option<String>,
    // 仅 Windows 有意义（miniblink DLL 路径）；Linux 忽略
    pub(crate) dll: Option<PathBuf>,
    pub(crate) web: Option<PathBuf>,
    pub(crate) port: Option<u16>,
}

fn main() {
    let opts = parse_args();
    log::init(platform::log_dir());
    // 平台壳接管：定位内核 → 起 HTTP 服务 → 建窗口 → 加载页面 → 消息循环，
    // 直到窗口关闭才返回。
    platform::run(opts);
    crate::logln!("[mb] 退出");
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
                     --dll <path>   指定 mb132_x64.dll 路径（仅 Windows）\n\
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

// ---------------------------------------------------------------- 配置持久化

/// 配置文件（落在用户数据目录）。窗口尺寸放在 `window` 段下，
/// 顶层还可扩展其它配置项（主题、启动页等），保存时只改 `window` 段、保留其余键。
pub(crate) fn config_path() -> PathBuf {
    platform::user_data_dir().join("config.json")
}

/// 读取整份配置（缺省返回空对象）。
///
/// 与 `save_geom` 共用 `config.json`：窗口尺寸在 `window` 段下，
/// 其它配置项（locale / theme 等）都在顶层，互不干扰。
pub(crate) fn read_config() -> Value {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

/// 把 patch 的顶层键合并进 config.json 并写回（保留 `window` 等其它段）。
///
/// 前端只 Patch `{ locale, theme }` 这类顶层键，所以顶层浅合并即可：
/// 既不会误删 `window` 段（窗口尺寸记忆），也不会要求前端传回整份配置。
pub(crate) fn merge_config(patch: &Value) -> Value {
    let path = config_path();
    let mut root = read_config();
    if !root.is_object() {
        root = Value::Object(serde_json::Map::new());
    }
    if let (Some(obj), Some(p)) = (root.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            obj.insert(k.clone(), v.clone());
        }
    }
    if let Ok(s) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(&path, s);
    }
    root
}

/// 写入窗口尺寸（`window` 段，读-改-写保留其它顶层键）。
/// Windows（WM_EXITSIZEMOVE / 退出）与 Linux（configure-event / delete-event）共用。
pub(crate) fn write_window_section(x: i32, y: i32, w: i32, h: i32) {
    let mut root = read_config();
    if !root.is_object() {
        root = Value::Object(serde_json::Map::new());
    }
    root["window"] = json!({ "x": x, "y": y, "w": w, "h": h });
    if let Ok(s) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(config_path(), s);
    }
}

/// 磁盘上前端产物目录（**仅开发期/未内嵌时用**），两个平台共用。
///
/// 优先级：exe 同级 → 项目内（cargo run）→ 当前工作目录。
/// 目录名沿用 vite 的 outDir（`build_dist`），同时认 `resources/` 作为备用，
/// 方便手工把产物放到别处测。
pub(crate) fn find_web_root() -> Option<PathBuf> {
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
    cands.into_iter().find(|p| p.join("index.html").exists())
}

#[cfg(test)]
mod config_tests {
    use super::*;

    /// user_data_dir 的 OnceLock 是进程级全局，用锁把用例串起来避免并发干扰。
    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// config.json 的「读-合并-写」必须只动被 patch 的顶层键，
    /// 保住 save_geom 写的 `window` 段（窗口尺寸记忆）不被前端刷掉。
    #[test]
    fn merge_config_round_trip_preserves_window() {
        let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        // 指到临时目录，绝不碰真实用户数据
        let tmp = std::env::temp_dir().join(format!("mbvue-cfg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("MB_USER_DATA", &tmp);

        // 该测试是本 crate 里唯一调用 user_data_dir 的用例，首次解析即命中 tmp
        assert_eq!(platform::user_data_dir(), tmp);

        // ① 先按 save_geom 的形态落一个 window 段
        let initial = json!({ "window": { "x": 10, "y": 20, "w": 1180, "h": 760 } });
        std::fs::write(config_path(), serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        // ② 前端 patch 顶层 locale / theme，window 必须原样保留
        let out = merge_config(&json!({ "locale": "en-US", "theme": "dark" }));
        assert_eq!(out["locale"], "en-US");
        assert_eq!(out["theme"], "dark");
        assert_eq!(out["window"]["w"], 1180);

        // ③ 再从盘上读回，确认落盘内容一致（前端 loadPersistedConfig 走这条路径）
        let back = read_config();
        assert_eq!(back["locale"], "en-US");
        assert_eq!(back["theme"], "dark");
        assert_eq!(back["window"]["x"], 10);
        assert_eq!(back["window"]["h"], 760);

        // ④ write_window_section 只动 window 段
        write_window_section(1, 2, 3, 4);
        let back = read_config();
        assert_eq!(back["window"]["x"], 1);
        assert_eq!(back["locale"], "en-US");

        // 清理现场
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::remove_var("MB_USER_DATA");
    }
}
