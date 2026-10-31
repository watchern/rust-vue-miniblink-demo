//! 轻量日志：GUI 子系统下没有控制台，日志同时写 stderr 和文件。
//!
//! 文件目录由平台壳决定（main 里 `log::init(platform::log_dir())`）：
//!   · Windows 固定 `<CARGO_MANIFEST_DIR>/logs/mbvue.log`（编译期确定，
//!     不随 exe 拷贝位置变化）；
//!   · Linux 写 `<user_data_dir>/mbvue.log`（deb 装到 /usr/bin 后，
//!     编译期路径既不存在也不可写）。
//! 每次启动截断重写，避免无限增长。

use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn init(dir: PathBuf) {
    let p = dir.join("mbvue.log");
    if let Some(parent) = p.parent() {
        let _ = create_dir_all(parent);
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
