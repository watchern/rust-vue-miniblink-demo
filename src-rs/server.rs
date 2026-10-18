//! 内置的本地 HTTP 服务：把 Vue 的构建产物喂给 miniblink。
//!
//! 为什么不用 file:// 直接加载：Vite 产物是 `<script type="module">`，
//! 在 file:// 协议下会被 CORS 拦掉；走 http://127.0.0.1 则一切正常，
//! 而且前端还能直接用 fetch 打本地接口。
//!
//! 资源来源有两档，由 build.rs 生成的 `mb_assets.rs` 决定：
//!   · **内嵌**（release 装了前端产物时）：资源编译进 exe，
//!     安装目录不用带 build_dist/，只留exe + core.dll + icon.ico。
//!   · **磁盘**（开发期或没构建前端时）：从 `root` 目录读，
//!     vite build 完刷新 exe 就能看到新产物，不用重新编译 Rust。

use crate::logln;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

// build.rs 生成的资源表（release 内嵌 / 开发期空表）。
// 这里必须用普通注释而非 doc comment：doc comment 会被解析成对下一个 item 的文档，
// 而下一个 item 是 include! 展开的代码 → rustc 报 `unused doc comment`。
include!(concat!(env!("OUT_DIR"), "/mb_assets.rs"));

/// 实际使用的资源源：内嵌表 or 磁盘目录
// （不是 doc comment：下面紧跟的 include! 会把它当成对下一个 item 的文档）
enum Source {
    /// 编译进 exe 的静态资源
    Embedded(&'static [(&'static str, &'static [u8])]),
    /// 磁盘目录（开发期）
    Disk(PathBuf),
}

impl Source {
    /// 取某个 URL 路径的内容。`None` = 没这个文件。
    ///
    /// 内嵌源直接返回编译期的 `&'static [u8]`（零拷贝）；
    /// 磁盘源每次读文件返回 `Cow`（开发期才走，不追求零拷贝）。
    fn get<'a>(&'a self, url: &str) -> Option<std::borrow::Cow<'a, [u8]>> {
        // 请求路径可能没前导斜杠，统一补齐再比
        let key = if url.starts_with('/') {
            url.to_string()
        } else {
            format!("/{}", url)
        };
        match self {
            Source::Embedded(t) => t
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| std::borrow::Cow::Borrowed(*v)),
            Source::Disk(root) => {
                let rel = url.trim_start_matches('/');
                let cand = safe_join(root, rel)?;
                if !cand.is_file() {
                    return None;
                }
                std::fs::read(cand).ok().map(std::borrow::Cow::Owned)
            }
        }
    }
}

/// 选资源源：内嵌表非空就用它，否则回退到磁盘目录。
/// `root` 只在回退到磁盘时用得上。
pub(crate) fn pick_source(root: PathBuf) -> Source {
    if MB_ASSETS_EMBEDDED && !MB_ASSETS.is_empty() {
        Source::Embedded(MB_ASSETS)
    } else {
        Source::Disk(root)
    }
}

/// 给 `app.info` 用的资源来源说明。
///
/// 内嵌时报「内嵌于 exe」，而不是把编译机的 build_dist 路径报出去——
/// 那个路径对终端用户毫无意义（甚至会暴露开发者本机目录）。
pub(crate) fn source_desc(src: &Source) -> String {
    match src {
        Source::Embedded(t) => format!("embedded in exe ({} files)", t.len()),
        Source::Disk(p) => p.display().to_string(),
    }
}

/// 进程内当前的资源来源（`spawn` 时记录，供 ipc 查询）
static SOURCE_DESC: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub(crate) fn source_desc_global() -> String {
    SOURCE_DESC.get().cloned().unwrap_or_default()
}

pub fn spawn(root: PathBuf) -> std::io::Result<u16> {
    let src = std::sync::Arc::new(pick_source(root));
    let desc = source_desc(&src);
    let _ = SOURCE_DESC.set(desc.clone());
    logln!("[http] 资源源: {desc}");
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    // 每连接克隆一份 Arc，'static 的要求就满足了
                    let src = std::sync::Arc::clone(&src);
                    std::thread::spawn(move || {
                        if let Err(e) = handle(s, &src) {
                            logln!("[http] {e}");
                        }
                    });
                }
                Err(e) => eprintln!("[http] accept 失败: {e}"),
            }
        }
    });

    Ok(port)
}

fn handle(mut stream: TcpStream, src: &Source) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(8)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;

    let mut reader = BufReader::new(&stream);
    let mut first = String::new();
    if reader.read_line(&mut first)? == 0 {
        return Ok(());
    }
    // 读掉剩余请求头，避免 RST
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
    }

    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");

    let path_only = target.split('?').next().unwrap_or("/");
    let path_only = percent_decode(path_only);
    let rel = path_only.trim_start_matches('/');

    // ---- 内置接口 ----
    if rel.starts_with("api/") {
        let body = api_response(&rel);
        return send(&mut stream, 200, "application/json; charset=utf-8", body.as_bytes());
    }

    if method != "GET" && method != "HEAD" {
        return send(&mut stream, 405, "text/plain", b"405 Method Not Allowed");
    }

    // ---- 静态资源 ----
    // 两段式：精确命中 → SPA 回退（无扩展名的请求给 index.html）→ 404。
    // 注意 SPA 回退时 mime 要按 index.html 算，不能按请求路径算
    //（比如 /dashboard 没有扩展名，回退后内容是 html，Content-Type 必须是 text/html）。
    let has_ext = rel.rsplit('/').next().map(|s| s.contains('.')).unwrap_or(false);
    let hit = src.get(rel).map(|b| (b, mime_of_url(rel)));
    let resolved = match hit {
        Some(h) => Some(h),
        None if !has_ext => src.get("/index.html").map(|b| (b, mime_of_url("index.html"))),
        None => None,
    };

    match resolved {
        Some((body, mime)) => send(&mut stream, 200, mime, &body),
        None => send(&mut stream, 404, "text/plain; charset=utf-8", b"404 Not Found"),
    }
}

fn api_response(rel: &str) -> String {
    match rel {
        "api/health" => {
            let pid = std::process::id();
            format!(
                r#"{{"ok":true,"pid":{pid},"service":"rust-vue-miniblink-demo","ts":{}}}"#,
                now_millis()
            )
        }
        "api/env" => {
            let root = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            serde_json::json!({
                "ok": true,
                "cwd": root,
                "pid": std::process::id(),
            })
            .to_string()
        }
        _ => r#"{"ok":false,"error":"unknown endpoint"}"#.to_string(),
    }
}

fn send(stream: &mut TcpStream, code: u16, mime: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        code,
        status_text(code),
        mime,
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(body)?;
    }
    stream.flush()
}

fn status_text(c: u16) -> &'static str {
    match c {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

/// 拼路径并阻止 `..` 穿越
fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() {
        return Some(root.join("index.html"));
    }
    let mut out = root.to_path_buf();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => return None,
            s => out.push(s),
        }
    }
    // 二次确认结果确实在 root 之下
    let canon_root = root.canonicalize().ok()?;
    let canon = out.canonicalize().ok()?;
    if canon.starts_with(&canon_root) {
        Some(canon)
    } else {
        None
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// 按 URL 路径判 mime（只看扩展名，不碰磁盘）。
fn mime_of_url(url: &str) -> &'static str {
    let ext = url
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(_, e)| e))
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "map" => "application/json; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_by_url_ext() {
        assert_eq!(mime_of_url("/index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_of_url("/assets/a-7ZymI7tF.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_of_url("/assets/a.css"), "text/css; charset=utf-8");
        assert_eq!(mime_of_url("/favicon.svg"), "image/svg+xml");
        assert_eq!(mime_of_url("/w.woff2"), "font/woff2");
        // 没有扩展名 → 走 default 分支
        assert_eq!(mime_of_url("/dashboard"), "application/octet-stream");
    }

    #[test]
    fn embedded_lookup_is_exact() {
        // 内嵌源按 key 精确匹配（不含 SPA 回退，那是 handle 的职责）
        let src = Source::Embedded(&[("/index.html", b"<html>" as &[u8]), ("/a.js", b"x")]);
        assert_eq!(src.get("/index.html").as_deref(), Some(&b"<html>"[..]));
        assert_eq!(src.get("index.html").as_deref(), Some(&b"<html>"[..])); // 补前导斜杠
        assert_eq!(src.get("/nope.js").is_none(), true);
    }

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("/a%20b.js"), "/a b.js");
        assert_eq!(percent_decode("/plain"), "/plain");
        // 不完整的 % 序列原样保留
        assert_eq!(percent_decode("/a%zz"), "/a%zz");
    }

    #[test]
    fn safe_join_blocks_traversal() {
        let root = std::env::temp_dir();
        assert!(safe_join(&root, "../escape").is_none());
        assert!(safe_join(&root, "a/../../escape").is_none());
    }

    #[test]
    fn source_desc_hides_build_machine_path() {
        // 内嵌源的描述绝不能是编译机的 build_dist 绝对路径，
        // 否则界面会向终端用户暴露开发者本机目录。
        let src = Source::Embedded(&[("/index.html", b"x")]);
        let d = source_desc(&src);
        assert!(d.starts_with("embedded in exe"), "实际: {d}");
        assert!(!d.contains(':'), "不该含盘符，实际: {d}");
    }

    #[test]
    fn pick_source_prefers_embedded() {
        // build.rs 生成的表非空时必须选内嵌（这正是安装版不需要 build_dist/ 的原因）
        let s = pick_source(PathBuf::from("."));
        if MB_ASSETS_EMBEDDED && !MB_ASSETS.is_empty() {
            assert!(matches!(s, Source::Embedded(_)), "有资源表却选了磁盘");
        } else {
            assert!(matches!(s, Source::Disk(_)), "空表却选了内嵌");
        }
    }
}
