//! JS ⇄ Rust 的命令分发层（**平台无关**）。
//!
//! 入站：两个平台壳各自把 JS 请求送进 `dispatch()`——
//!   Windows: miniblink `onJsQuery` 回调（platform_win.rs），
//!   Linux:   WebKitGTK script-message-handler 信号（platform_linux.rs）。
//! 出站：任意线程调用 `emit()`，经 `platform::eval_js` 把事件推到 `window.__mbEmit`。
//! 窗口类命令（win.*）一律走 `platform` 门面，这里不出现 Win32 / GTK 代码。

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::logln;
use crate::platform;

// ---------------------------------------------------------------- 运行时状态

static VIEW: AtomicI64 = AtomicI64::new(0);
static PORT: AtomicU32 = AtomicU32::new(0);
/// Windows：miniblink DLL 路径；Linux：内核描述串（WebKitGTK 无独立内核文件）
static DLL_PATH: OnceLock<String> = OnceLock::new();
/// 回退用的磁盘前端目录（仅开发期 / 未内嵌时有意义）。
/// `app.info` 报给界面的不是这个——那个用 `server::source_desc_global()`，
/// 免得把编译机的 build_dist 绝对路径暴露给终端用户。
static WEB_ROOT: OnceLock<String> = OnceLock::new();
static STARTED_AT: OnceLock<u128> = OnceLock::new();

pub fn init_runtime(view: i64, port: u16, kernel: String, web_root: String) {
    VIEW.store(view, Ordering::SeqCst);
    PORT.store(port as u32, Ordering::SeqCst);
    let _ = DLL_PATH.set(kernel);
    let _ = WEB_ROOT.set(web_root);
    let _ = STARTED_AT.set(now_millis());
}

pub fn current_view() -> i64 {
    VIEW.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------- 出站事件

/// 把事件推给前端：`window.__mbEmit({event, data, ts})`
pub fn emit(event: &str, data: Value) {
    let view = current_view();
    if view == 0 {
        return;
    }
    let payload = json!({
        "event": event,
        "data": data,
        "ts": now_millis(),
    });
    let js = format!("window.__mbEmit({})", payload);
    if std::env::var("MB_TRACE").is_ok() {
        logln!("[emit] {}", &js[..js.len().min(140)]);
    }
    // 派发到 webview：Windows 经 mbRunJs，Linux 经 evaluate_javascript（主线程）
    platform::eval_js(js);
}

// ---------------------------------------------------------------- 入站分发

pub fn dispatch(cmd: &str, payload: &Value) -> Value {
    let r = run(cmd, payload);
    match r {
        Ok(v) => json!({ "ok": true, "data": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

fn get_str<'a>(p: &'a Value, k: &str, d: &'a str) -> &'a str {
    p.get(k).and_then(|v| v.as_str()).unwrap_or(d)
}

fn get_i64(p: &Value, k: &str, d: i64) -> i64 {
    // 兼容 number 与 "123" 字符串两种传法
    p.get(k).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(d)
}

fn run(cmd: &str, p: &Value) -> Result<Value, String> {
    match cmd {
        // ---------------- 应用 ----------------
        "app.info" => Ok(json!({
            "name": "rust-vue-miniblink-demo",
            "version": env!("CARGO_PKG_VERSION"),
            "engine": crate::ENGINE_NAME,
            "dll": DLL_PATH.get().cloned().unwrap_or_default(),
            // 资源来源：内嵌时报「内嵌于 exe」，不报编译机的 build_dist 路径
            // （那个路径对终端用户无意义，还会暴露开发者本机目录）。
            "webRoot": crate::server::source_desc_global(),
            "userData": platform::user_data_dir().display().to_string(),
            "localPort": PORT.load(Ordering::SeqCst),
            "pid": std::process::id(),
            "arch": std::env::consts::ARCH,
            "uptimeMs": now_millis() - STARTED_AT.get().copied().unwrap_or(0),
            "rustc": rustc_version(),
            "locale": crate::i18n::current(),
        })),
        // 前端切换语种时同步过来，让壳侧返回的错误文案也跟着变
        "app.setLocale" => {
            let cur = crate::i18n::set_locale(get_str(p, "locale", "zh-CN"));
            Ok(json!({ "locale": cur }))
        }
        // 前端主题生效时同步过来：Windows 端给无边框窗口那圈兜底背景上色
        // （启动瞬间/缩放过程中 webview 未及时重画时不闪浅色）；Linux 无需处理。
        "app.setTheme" => {
            let th = get_str(p, "theme", "light");
            platform::set_theme(th == "dark");
            Ok(json!({ "theme": th }))
        }

        // ---------------- 配置（持久化到 config.json）----------------
        // 前端把界面语种 / 主题这类"跨重启保留"的项落盘到这里，
        // 顶层键合并写入，保留 window 段（窗口尺寸记忆）等其它配置。
        "config.get" => Ok(crate::read_config()),
        "config.set" => Ok(crate::merge_config(p)),

        // ---------------- 系统 ----------------
        "sys.info" => sys_info(),
        "sys.memory" => {
            let mut s = sysinfo::System::new();
            s.refresh_memory();
            Ok(json!({
                "total": s.total_memory(),
                "used": s.used_memory(),
                "available": s.available_memory(),
                "swapTotal": s.total_swap(),
                "swapUsed": s.used_swap(),
            }))
        }

        // ---------------- 窗口（全部走平台门面）----------------
        "win.minimize" => {
            // Windows：收进托盘（隐藏窗口），托盘图标是恢复/退出的入口；
            // Linux：无托盘实现，语义为最小化到任务栏。
            platform::win_minimize()?;
            Ok(json!({ "action": "minimize", "ok": true }))
        }
        "win.maximize" => {
            let maximized = platform::win_maximize()?;
            Ok(json!({ "maximized": maximized }))
        }
        "win.close" => {
            platform::win_close()?;
            Ok(json!({ "action": "close", "ok": true }))
        }
        "win.drag" => {
            // 无边框窗口拖拽：Windows 伪造标题栏按下，Linux 走 begin_move_drag
            platform::win_drag()?;
            Ok(json!({ "action": "drag", "ok": true }))
        }
        "win.title" => {
            let t = get_str(p, "title", "");
            platform::win_set_title(t)?;
            Ok(json!({ "title": t }))
        }
        "win.getSize" => {
            // 前端 resize handle 拖拽时读基准尺寸
            let (w, h) = platform::win_get_size()?;
            Ok(json!({ "w": w, "h": h }))
        }
        "win.resize" => {
            // 钳到 [最小, 16384]；宽高独立钳制，不动的维度不受影响。
            // 缺参数（<=0）时保持原样，不回退到最小尺寸。
            let rw = get_i64(p, "w", 0);
            let rh = get_i64(p, "h", 0);
            let w = (rw as i32).clamp(crate::WIN_MIN_W, 16384);
            let h = (rh as i32).clamp(crate::WIN_MIN_H, 16384);
            if rw > 0 && rh > 0 {
                platform::win_resize(w, h)?;
            }
            Ok(json!({ "w": w, "h": h }))
        }

        // ---------------- 文件 ----------------
        "fs.drives" => fs_drives(),
        "fs.list" => fs_list(get_str(p, "path", ".")),
        "fs.read" => fs_read(get_str(p, "path", ""), 256 * 1024),
        "fs.write" => fs_write(get_str(p, "path", ""), get_str(p, "content", "")),

        // ---------------- 演示 ----------------
        "echo" => Ok(json!({
            "echo": p.get("message").cloned().unwrap_or(Value::Null),
            "at": now_millis(),
        })),
        "compute.fib" => {
            let n = p.get("n").and_then(|v| v.as_u64()).unwrap_or(30).min(40);
            let t0 = std::time::Instant::now();
            let v = fib(n);
            Ok(json!({ "n": n, "result": v, "elapsedUs": t0.elapsed().as_micros() }))
        }
        "compute.hash" => {
            // 稍微吃 CPU 的活儿，用来体现原生侧能力
            let rounds = p.get("rounds").and_then(|v| v.as_u64()).unwrap_or(300_000).min(5_000_000);
            let t0 = std::time::Instant::now();
            let mut h: u64 = 0xcbf29ce484222325;
            for i in 0..rounds {
                h ^= i.wrapping_mul(0x9E3779B97F4A7C15);
                h = h.wrapping_mul(0x100000001B3);
            }
            Ok(json!({ "rounds": rounds, "hash": format!("{:016x}", h), "elapsedUs": t0.elapsed().as_micros() }))
        }

        // ---------------- 事件流 ----------------
        "event.start" => {
            let interval = p.get("intervalMs").and_then(|v| v.as_u64()).unwrap_or(1000).clamp(100, 60_000);
            start_ticker(interval);
            Ok(json!({ "started": true, "intervalMs": interval }))
        }
        "event.stop" => {
            TICKER.store(false, Ordering::SeqCst);
            Ok(json!({ "started": false }))
        }

        _ => Err(crate::i18n::unknown_cmd(cmd)),
    }
}

// ---------------------------------------------------------------- 系统信息

fn sys_info() -> Result<Value, String> {
    use sysinfo::{Disks, System};

    let mut s = System::new_all();
    // CPU 占用需要采样间隔
    std::thread::sleep(std::time::Duration::from_millis(220));
    s.refresh_cpu_all();

    let cpus: Vec<Value> = s
        .cpus()
        .iter()
        .map(|c| json!({ "brand": c.brand().to_string(), "freqMHz": c.frequency(), "usage": c.cpu_usage() }))
        .collect();

    let disks: Vec<Value> = Disks::new_with_refreshed_list()
        .iter()
        .map(|d| {
            json!({
                "name": d.name().to_string_lossy(),
                "mount": d.mount_point().to_string_lossy(),
                "fs": d.file_system().to_string_lossy(),
                "total": d.total_space(),
                "available": d.available_space(),
                "removable": d.is_removable(),
            })
        })
        .collect();

    Ok(json!({
        "host": System::host_name().unwrap_or_default(),
        "os": System::name().unwrap_or_default(),
        "osVersion": System::os_version().unwrap_or_default(),
        "kernel": System::kernel_version().unwrap_or_default(),
        "arch": std::env::consts::ARCH,
        "cpuCount": s.cpus().len(),
        "cpuUsage": s.global_cpu_usage(),
        "cpus": cpus,
        "memTotal": s.total_memory(),
        "memUsed": s.used_memory(),
        "memAvailable": s.available_memory(),
        "disks": disks,
        "uptimeSec": System::uptime(),
        "pid": std::process::id(),
    }))
}

// ---------------------------------------------------------------- 文件操作

/// 盘符列表：Windows 扫 A:-Z:；Linux 没有盘符概念，列挂载点（对齐 sysinfo 的 Disks）。
fn fs_drives() -> Result<Value, String> {
    #[cfg(windows)]
    {
        let mut out = Vec::new();
        for b in b'A'..=b'Z' {
            let root = format!("{}:\\", b as char);
            if std::path::Path::new(&root).exists() {
                out.push(json!({ "name": root, "path": root }));
            }
        }
        Ok(json!(out))
    }
    #[cfg(not(windows))]
    {
        use sysinfo::Disks;
        let mut out: Vec<Value> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for d in Disks::new_with_refreshed_list().iter() {
            let mp = d.mount_point().to_string_lossy().to_string();
            if seen.iter().any(|s| mp.starts_with(s.as_str())) {
                continue; // 子挂载点不重复列
            }
            seen.push(mp.clone());
            let name = d.name().to_string_lossy().to_string();
            out.push(json!({
                "name": if name.is_empty() { mp.clone() } else { name },
                "path": mp,
            }));
        }
        Ok(json!(out))
    }
}

fn fs_list(path: &str) -> Result<Value, String> {
    let base = resolve(path)?;
    let rd = std::fs::read_dir(&base).map_err(|e| crate::i18n::read_dir_failed(&e.to_string()))?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in rd.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        let item = json!({
            "name": name,
            "path": entry.path().to_string_lossy(),
            "isDir": meta.is_dir(),
            "size": meta.len(),
            "modified": meta.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()).unwrap_or(0),
        });
        if meta.is_dir() {
            dirs.push(item);
        } else {
            files.push(item);
        }
    }
    dirs.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_lowercase());
    files.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_lowercase());

    Ok(json!({
        "path": base.to_string_lossy(),
        "parent": base.parent().map(|p| p.to_string_lossy().to_string()),
        "dirs": dirs,
        "files": files,
    }))
}

fn fs_read(path: &str, limit: usize) -> Result<Value, String> {
    let p = resolve(path)?;
    let meta = std::fs::metadata(&p).map_err(|e| format!("{e}"))?;
    if meta.len() > limit as u64 * 4 {
        return Err(crate::i18n::file_too_large(meta.len()));
    }
    let bytes = std::fs::read(&p).map_err(|e| crate::i18n::read_failed(&e.to_string()))?;
    let truncated = bytes.len() > limit;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(limit)]).to_string();
    Ok(json!({
        "path": p.to_string_lossy(),
        "size": meta.len(),
        "truncated": truncated,
        "content": text,
    }))
}

fn fs_write(path: &str, content: &str) -> Result<Value, String> {
    let p = resolve(path)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| crate::i18n::mkdir_failed(&e.to_string()))?;
    }
    std::fs::write(&p, content).map_err(|e| crate::i18n::write_failed(&e.to_string()))?;
    Ok(json!({ "path": p.to_string_lossy(), "bytes": content.len() }))
}

fn resolve(path: &str) -> Result<std::path::PathBuf, String> {
    if path.is_empty() {
        return Err(crate::i18n::empty_path());
    }
    let p = std::path::PathBuf::from(path);
    let p = if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) };
    // 归一化（不做 canonicalize，避免要求文件必须存在）
    let mut out = std::path::PathBuf::new();
    for seg in p.components() {
        match seg {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- 定时器

static TICKER: AtomicBool = AtomicBool::new(false);

fn start_ticker(interval_ms: u64) {
    TICKER.store(true, Ordering::SeqCst);
    std::thread::spawn(move || {
        let mut n: u64 = 0;
        while TICKER.load(Ordering::SeqCst) {
            n += 1;
            let mut s = sysinfo::System::new();
            s.refresh_memory();
            s.refresh_cpu_all();
            emit(
                "tick",
                json!({
                    "seq": n,
                    "memUsed": s.used_memory(),
                    "memTotal": s.total_memory(),
                    "cpu": s.global_cpu_usage(),
                }),
            );
            std::thread::sleep(std::time::Duration::from_millis(interval_ms));
        }
        emit("tick-stopped", json!({ "seq": n }));
    });
}

// ---------------------------------------------------------------- 工具

fn fib(n: u64) -> u128 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a: u128 = 0;
            let mut b: u128 = 1;
            for _ in 2..=n {
                let t = a + b;
                a = b;
                b = t;
            }
            b
        }
    }
}

fn rustc_version() -> String {
    option_env!("MB_RUSTC").unwrap_or("stable").to_string()
}

pub fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
