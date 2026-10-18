//! miniblink (mb132) 的 Rust FFI 绑定。
//!
//! 所有符号都在运行时通过 `libloading` 从 `mb132_x64.dll` 解析，
//! 因此不需要 .lib 导入库，也无需链接器配置。
//!
//! 类型对照（来自 vendor/miniblink/demo_src/mb.h）：
//!   mbWebView        = __int64 / intptr_t   -> i64
//!   mbJsValue        = long long            -> i64
//!   mbJsExecState    = void*                -> *mut c_void
//!   mbWebFrameHandle = void*                -> *mut c_void
//!   HWND             = void*                -> *mut c_void
//!   BOOL             = int                  -> i32
//!   DWORD            = unsigned long        -> u32
//!   utf8             = char                 -> c_char
//!
//! 调用约定：MSVC x64 下 MB_CALL_TYPE 展开为空，即统一的 x64 调用约定，
//! 这里用 `extern "system"` 描述（x64 上与 "C" 等价，x86 下等价于 __stdcall）。

// 字段名保持 miniblink 原始的 camelCase 命名，便于和 mb.h 对照
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::sync::OnceLock;

pub type MbWebView = i64;
pub type MbJsValue = i64;
pub type MbJsExecState = *mut c_void;
pub type MbWebFrameHandle = *mut c_void;
pub type MbHwnd = *mut c_void;
pub type MbBool = i32;
pub type MbDword = u32;

/// JS -> Rust：`window.mbQuery(customMsg, request, cb)` 触发
pub type MbJsQueryCallback =
    unsafe extern "system" fn(MbWebView, *mut c_void, MbJsExecState, i64, c_int, *const c_char);
/// `mbRunJs` 执行完毕的回调
pub type MbRunJsCallback = unsafe extern "system" fn(MbWebView, *mut c_void, MbJsExecState, MbJsValue);
/// 文档就绪
pub type MbDocumentReadyCallback = unsafe extern "system" fn(MbWebView, *mut c_void, MbWebFrameHandle);

pub struct Mb {
    // 必须持有 Library，否则函数指针会悬空
    _lib: libloading::Library,

    pub mbInit: unsafe extern "system" fn(*const c_void),
    pub mbUninit: unsafe extern "system" fn(),
    pub mbEnableHighDPISupport: unsafe extern "system" fn(),
    pub mbRunMessageLoop: unsafe extern "system" fn(),
    // 主动退出 miniblink 自己的消息循环（X 关闭 / 托盘退出时调用）
    pub mbExitMessageLoop: unsafe extern "system" fn(),

    pub mbCreateWebCustomWindow: unsafe extern "system" fn(
        MbHwnd, MbDword, MbDword, c_int, c_int, c_int, c_int,
    ) -> MbWebView,
    pub mbCreateWebWindow:
        unsafe extern "system" fn(c_int, MbHwnd, c_int, c_int, c_int, c_int) -> MbWebView,
    pub mbDestroyWebView: unsafe extern "system" fn(MbWebView),
    pub mbGetHostHWND: unsafe extern "system" fn(MbWebView) -> MbHwnd,
    pub mbSetHandle: unsafe extern "system" fn(MbWebView, MbHwnd),
    pub mbShowWindow: unsafe extern "system" fn(MbWebView, c_int),
    pub mbSetWindowTitle: unsafe extern "system" fn(MbWebView, *const c_char),
    pub mbSetUserAgent: unsafe extern "system" fn(MbWebView, *const c_char),
    pub mbSetDebugConfig: unsafe extern "system" fn(MbWebView, *const c_char, *const c_char),
    pub mbSetNavigationToNewWindowEnable: unsafe extern "system" fn(MbWebView, MbBool),
    pub mbSetCspCheckEnable: unsafe extern "system" fn(MbWebView, MbBool),
    // ---- 运行时数据落盘位置（宽字符路径，UTF-16）----
    // 不设的话 miniblink 默认写在 exe 同级目录：装到 Program Files 下时
    // 那是个需要管理员权限的位置，且会把安装目录弄脏。
    pub mbSetLocalStorageFullPath: unsafe extern "system" fn(MbWebView, *const u16),
    pub mbSetCookieJarFullPath: unsafe extern "system" fn(MbWebView, *const u16),
    pub mbSetDiskCachePath: unsafe extern "system" fn(MbWebView, *const u16),
    pub mbSetDiskCacheEnabled: unsafe extern "system" fn(MbWebView, MbBool),
    pub mbSetDragEnable: unsafe extern "system" fn(MbWebView, MbBool),
    pub mbSetDragDropEnable: unsafe extern "system" fn(MbWebView, MbBool),

    pub mbLoadURL: unsafe extern "system" fn(MbWebView, *const c_char),

    pub mbWebFrameGetMainFrame: unsafe extern "system" fn(MbWebView) -> MbWebFrameHandle,
    pub mbOnDocumentReady: unsafe extern "system" fn(MbWebView, MbDocumentReadyCallback, *mut c_void),
    pub mbOnJsQuery: unsafe extern "system" fn(MbWebView, MbJsQueryCallback, *mut c_void),
    pub mbResponseQuery: unsafe extern "system" fn(MbWebView, i64, c_int, *const c_char),
    pub mbRunJs: unsafe extern "system" fn(
        MbWebView,
        MbWebFrameHandle,
        *const c_char,
        MbBool,
        MbRunJsCallback,
        *mut c_void,
        *mut c_void,
    ),
    pub mbRunJsSync:
        unsafe extern "system" fn(MbWebView, MbWebFrameHandle, *const c_char, MbBool) -> MbJsValue,
    pub mbJsToString: unsafe extern "system" fn(MbJsExecState, MbJsValue) -> *const c_char,
}

static MB: OnceLock<Mb> = OnceLock::new();

pub fn mb() -> &'static Mb {
    MB.get().expect("miniblink 尚未加载，请先调用 ffi::load()")
}

pub fn is_loaded() -> bool {
    MB.get().is_some()
}

/// 加载 miniblink DLL 并解析全部符号
pub fn load(dll: &Path) -> Result<(), String> {
    let lib = unsafe { libloading::Library::new(dll) }
        .map_err(|e| format!("加载 {} 失败: {e}", dll.display()))?;

    macro_rules! sym {
        ($n:literal, $t:ty) => {{
            let s: libloading::Symbol<$t> = unsafe { lib.get(concat!($n, "\0").as_bytes()) }
                .map_err(|e| format!("解析符号 {} 失败: {e}", $n))?;
            *s
        }};
    }

    let api = Mb {
        mbInit: sym!("mbInit", unsafe extern "system" fn(*const c_void)),
        mbUninit: sym!("mbUninit", unsafe extern "system" fn()),
        mbEnableHighDPISupport: sym!("mbEnableHighDPISupport", unsafe extern "system" fn()),
        mbRunMessageLoop: sym!("mbRunMessageLoop", unsafe extern "system" fn()),
        mbExitMessageLoop: sym!("mbExitMessageLoop", unsafe extern "system" fn()),

        mbCreateWebCustomWindow: sym!(
            "mbCreateWebCustomWindow",
            unsafe extern "system" fn(MbHwnd, MbDword, MbDword, c_int, c_int, c_int, c_int) -> MbWebView
        ),
        mbCreateWebWindow: sym!(
            "mbCreateWebWindow",
            unsafe extern "system" fn(c_int, MbHwnd, c_int, c_int, c_int, c_int) -> MbWebView
        ),
        mbDestroyWebView: sym!("mbDestroyWebView", unsafe extern "system" fn(MbWebView)),
        mbGetHostHWND: sym!("mbGetHostHWND", unsafe extern "system" fn(MbWebView) -> MbHwnd),
        mbSetHandle: sym!("mbSetHandle", unsafe extern "system" fn(MbWebView, MbHwnd)),
        mbShowWindow: sym!("mbShowWindow", unsafe extern "system" fn(MbWebView, c_int)),
        mbSetWindowTitle: sym!("mbSetWindowTitle", unsafe extern "system" fn(MbWebView, *const c_char)),
        mbSetUserAgent: sym!("mbSetUserAgent", unsafe extern "system" fn(MbWebView, *const c_char)),
        mbSetDebugConfig: sym!(
            "mbSetDebugConfig",
            unsafe extern "system" fn(MbWebView, *const c_char, *const c_char)
        ),
        mbSetNavigationToNewWindowEnable: sym!(
            "mbSetNavigationToNewWindowEnable",
            unsafe extern "system" fn(MbWebView, MbBool)
        ),
        mbSetCspCheckEnable: sym!("mbSetCspCheckEnable", unsafe extern "system" fn(MbWebView, MbBool)),
        mbSetLocalStorageFullPath: sym!(
            "mbSetLocalStorageFullPath",
            unsafe extern "system" fn(MbWebView, *const u16)
        ),
        mbSetCookieJarFullPath: sym!(
            "mbSetCookieJarFullPath",
            unsafe extern "system" fn(MbWebView, *const u16)
        ),
        mbSetDiskCachePath: sym!(
            "mbSetDiskCachePath",
            unsafe extern "system" fn(MbWebView, *const u16)
        ),
        mbSetDiskCacheEnabled: sym!(
            "mbSetDiskCacheEnabled",
            unsafe extern "system" fn(MbWebView, MbBool)
        ),
        mbSetDragEnable: sym!("mbSetDragEnable", unsafe extern "system" fn(MbWebView, MbBool)),
        mbSetDragDropEnable: sym!("mbSetDragDropEnable", unsafe extern "system" fn(MbWebView, MbBool)),

        mbLoadURL: sym!("mbLoadURL", unsafe extern "system" fn(MbWebView, *const c_char)),

        mbWebFrameGetMainFrame: sym!(
            "mbWebFrameGetMainFrame",
            unsafe extern "system" fn(MbWebView) -> MbWebFrameHandle
        ),
        mbOnDocumentReady: sym!(
            "mbOnDocumentReady",
            unsafe extern "system" fn(MbWebView, MbDocumentReadyCallback, *mut c_void)
        ),
        mbOnJsQuery: sym!(
            "mbOnJsQuery",
            unsafe extern "system" fn(MbWebView, MbJsQueryCallback, *mut c_void)
        ),
        mbResponseQuery: sym!(
            "mbResponseQuery",
            unsafe extern "system" fn(MbWebView, i64, c_int, *const c_char)
        ),
        mbRunJs: sym!(
            "mbRunJs",
            unsafe extern "system" fn(MbWebView, MbWebFrameHandle, *const c_char, MbBool, MbRunJsCallback, *mut c_void, *mut c_void)
        ),
        mbRunJsSync: sym!(
            "mbRunJsSync",
            unsafe extern "system" fn(MbWebView, MbWebFrameHandle, *const c_char, MbBool) -> MbJsValue
        ),
        mbJsToString: sym!(
            "mbJsToString",
            unsafe extern "system" fn(MbJsExecState, MbJsValue) -> *const c_char
        ),

        _lib: lib,
    };

    MB.set(api).map_err(|_| "miniblink 已加载过".to_string())?;
    Ok(())
}

/// 转成以 NUL 结尾的 C 字符串（调用方需保证 Vec 存活到 FFI 调用结束）
pub fn cstr(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}
