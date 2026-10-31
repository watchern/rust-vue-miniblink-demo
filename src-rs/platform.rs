//! 平台门面（facade）。
//!
//! main.rs / ipc.rs 只通过这里调用平台能力（窗口操作、JS 推送、用户数据目录等），
//! 不直接依赖 windows_sys 或 GTK。两个实现导出**同名同签名**的函数集合：
//!
//!   · platform_win.rs   —— Windows：miniblink DLL + Win32（无边框/托盘/原生缩放）
//!   · platform_linux.rs —— Linux：GTK3 + WebKitGTK 4.1（libsoup3）
//!
//! 约定的导出面（缺一项另一个平台必须补齐，编译期由 cfg 分发保证）：
//!   run(opts)                        阻塞直到窗口关闭
//!   eval_js(js: String)              把 JS 片段派发到 webview 执行（fire-and-forget）
//!   set_theme(dark: bool)            主题同步（Windows 涂背景兜底色；Linux no-op）
//!   win_minimize / win_maximize / win_close / win_drag
//!   win_get_size / win_resize / win_set_title
//!   user_data_dir() / log_dir()

#[cfg(windows)]
pub(crate) use crate::platform_win::*;
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) use crate::platform_linux::*;
