//! 壳侧的文案表。
//!
//! 为什么壳也要有一份：miniblink 的页面与 Rust 是两个运行时，
//! 前端拿不到 Rust 的模块。IPC 报错（如「读取目录失败」）会直接显示在界面错误栏里，
//! 界面上显示中文而英文界面下报错仍是中文会很突兀，所以这里按前端传来的语种返回对应文案。
//!
//! key 与 src/i18n/index.ts 保持同名同义，两边可对照。
//! 注意 key 集合不必完全一致：这里只管壳会发出去的那几条，前端表是完整界面。

use std::sync::atomic::{AtomicU8, Ordering};

pub const ZH_CN: u8 = 0;
pub const EN_US: u8 = 1;

/// 当前语种（由前端在每次调用 app.setLocale 时同步过来）。
static LOCALE: AtomicU8 = AtomicU8::new(ZH_CN);

pub fn set_locale(tag: &str) -> &'static str {
    let next = match tag {
        "en-US" => EN_US,
        "zh-CN" => ZH_CN,
        // 认不出的标签保持现状，别把界面踢回默认语言
        _ => return current(),
    };
    LOCALE.store(next, Ordering::Relaxed);
    current()
}

pub fn current() -> &'static str {
    if LOCALE.load(Ordering::Relaxed) == EN_US {
        "en-US"
    } else {
        "zh-CN"
    }
}

fn is_en() -> bool {
    LOCALE.load(Ordering::Relaxed) == EN_US
}

/// 按当前语种取文案（不存在的一律回中文，再不回落到 key）。
fn t(zh: &str, en: &str) -> String {
    if is_en() { en.to_string() } else { zh.to_string() }
}

/// 带参数的文案。`{0}` 之类的占位由 fmt_args 风格的替换实现（避免引入 format! 之外的新依赖）。
fn tf(zh: &str, en: &str, args: &[&str]) -> String {
    let tpl = t(zh, en);
    let mut out = tpl;
    for (i, a) in args.iter().enumerate() {
        let needle = format!("{{{i}}}");
        out = out.replace(&needle, a);
    }
    out
}

// ---------------------------------------------------------------- 各处错误文案

pub fn unknown_cmd(cmd: &str) -> String {
    tf("未知命令: {0}", "Unknown command: {0}", &[cmd])
}

pub fn read_dir_failed(e: &str) -> String {
    tf("读取目录失败: {0}", "Failed to read directory: {0}", &[e])
}

pub fn file_too_large(bytes: u64) -> String {
    let s = bytes.to_string();
    tf(
        "文件过大（{0} 字节），已跳过",
        "File is too large ({0} bytes), skipped",
        &[&s],
    )
}

pub fn read_failed(e: &str) -> String {
    tf("读取失败: {0}", "Read failed: {0}", &[e])
}

pub fn mkdir_failed(e: &str) -> String {
    tf("建目录失败: {0}", "Failed to create directory: {0}", &[e])
}

pub fn write_failed(e: &str) -> String {
    tf("写入失败: {0}", "Write failed: {0}", &[e])
}

pub fn empty_path() -> String {
    t("路径为空", "Path is empty")
}

pub fn json_parse_failed(e: &str) -> String {
    tf("JSON 解析失败: {0}", "Failed to parse JSON: {0}", &[e])
}

pub fn webview_not_ready() -> String {
    t("webview 尚未就绪", "The webview is not ready yet")
}

pub fn no_hwnd() -> String {
    t("拿不到窗口句柄", "Cannot get the window handle")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// LOCALE 是进程级全局状态，而 cargo test 默认把用例并行跑在多个线程上，
    /// 不加锁会出现「用例 A 设了 en-US、用例 B 却在断言中文」的偶发失败。
    /// 用一把静态锁把每个用例整体串起来（用例本身是纯内存操作，串行不慢）。
    static GUARD: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        // 中毒了就当已锁继续跑：这里没有跨用例的状态依赖，忽略 poisoning 更实用
        GUARD.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 壳侧报错文案必须随语种变化 —— 这是「界面切英文后 IPC 报错仍是中文」的防线。
    #[test]
    fn shell_messages_follow_locale() {
        let _g = lock();
        set_locale("zh-CN");
        assert_eq!(current(), "zh-CN");
        assert_eq!(unknown_cmd("foo.bar"), "未知命令: foo.bar");
        assert_eq!(empty_path(), "路径为空");
        assert_eq!(no_hwnd(), "拿不到窗口句柄");

        // 切到英文后，同一批报错必须换成英文
        set_locale("en-US");
        assert_eq!(current(), "en-US");
        assert_eq!(unknown_cmd("foo.bar"), "Unknown command: foo.bar");
        assert_eq!(empty_path(), "Path is empty");
        assert_eq!(no_hwnd(), "Cannot get the window handle");
        assert_eq!(read_dir_failed("boom"), "Failed to read directory: boom");
        assert_eq!(
            file_too_large(1234),
            "File is too large (1234 bytes), skipped"
        );
        assert_eq!(json_parse_failed("bad"), "Failed to parse JSON: bad");
        assert_eq!(webview_not_ready(), "The webview is not ready yet");
    }

    #[test]
    fn unknown_locale_tag_keeps_current() {
        let _g = lock();
        set_locale("zh-CN");
        assert_eq!(set_locale("fr-FR"), "zh-CN"); // 认不出的标签不踢回默认
        set_locale("en-US");
        assert_eq!(set_locale(""), "en-US");
    }

    #[test]
    fn placeholders_are_substituted() {
        let _g = lock();
        set_locale("en-US");
        assert!(write_failed("disk full").contains("disk full"));
        assert!(!write_failed("disk full").contains('{'), "占位符应被替换掉");
        set_locale("zh-CN");
        assert!(read_failed("io").contains("读取失败"));
    }
}
