//! 轻量日志：GUI 子系统下没有控制台，日志同时写 stderr 和文件。
//!
//! 文件位置固定为 `<CARGO_MANIFEST_DIR>/logs/mbvue.log`（编译期确定，
//! 不随 exe 拷贝位置变化），每次启动截断重写，避免无限增长。

use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn log_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/logs/mbvue.log"))
}

pub fn init() {
    let p = log_path();
    if let Some(dir) = p.parent() {
        let _ = create_dir_all(dir);
    }
    match File::create(&p) {
        Ok(f) => *LOG_FILE.lock().unwrap() = Some(f),
        Err(e) => eprintln!("[log] 打不开 {}: {e}", p.display()),
    }
}

pub fn line(msg: impl AsRef<str>) {
    let s = msg.as_ref();
    // stderr 在 GUI 子系统下写不出去，忽略错误即可
    let _ = writeln!(std::io::stderr(), "{s}");
    if let Ok(mut g) = LOG_FILE.lock() {
        if let Some(f) = g.as_mut() {
            let _ = writeln!(f, "{s}");
            let _ = f.flush();
        }
    }
}

macro_rules! logln {
    ($($arg:tt)*) => { $crate::log::line(format!($($arg)*)) };
}

pub(crate) use logln;
