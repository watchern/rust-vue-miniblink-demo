//! 把 miniblink 运行时 DLL 拷贝到 cargo 的产物目录（target/<profile>/），
//! 这样 `cargo run` 出来的 exe 能直接找到 core.dll（开发期名 mb132_x64.dll）。
//!
//! DLL 本体（68MB）放在 vendor/miniblink/ 下，不经 git 追踪，
//! 首次获取方式见 scripts/fetch-miniblink.sh。
//!
//! 另外在**编译期**把 `build_dist/` 里的前端产物生成一份资源表
//! （`$OUT_DIR/mb_assets.rs`），供 server.rs `include!`：
//! release 版 exe 自带前端，装完只剩 exe + dll + icon，不用带一整个目录。

use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // ---- 前端产物：生成内嵌资源表 ----
    emit_asset_table(&manifest, &out_dir);

    // ---- miniblink DLL 部署到 cargo 产物目录 ----
    let dll_names = ["mb132_x64.dll", "mb132_x32.dll"];

    // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out
    // 往上 3 级即 <target>/<profile>
    let Some(target_dir) = out_dir.ancestors().nth(3) else {
        return;
    };

    for name in dll_names {
        let src = manifest.join("vendor").join("miniblink").join(name);
        if !src.exists() {
            continue;
        }
        let dst = target_dir.join(name);
        // 已存在且大小一致就跳过，避免每次构建重复拷贝 68MB
        if dst.exists()
            && dst.metadata().map(|m| m.len()).unwrap_or(0) == src.metadata().map(|m| m.len()).unwrap_or(1)
        {
            continue;
        }
        if let Err(e) = std::fs::copy(&src, &dst) {
            println!("cargo:warning=拷贝 {} 失败: {}", name, e);
        } else {
            println!("cargo:warning=已部署 {} -> {}", name, dst.display());
        }
    }

    // 前端产物变化时无需重新编译 Rust，这里只跟踪头文件
    let header = manifest.join("vendor").join("miniblink").join("demo_src").join("mb.h");
    if header.exists() {
        println!("cargo:rerun-if-changed={}", header.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    // 让代码里能拿到部署后的 DLL 目录，作为运行时搜索兜底
    println!("cargo:rustc-env=MB_TARGET_DIR={}", Path::new(target_dir).display());

    // ---- Windows 图标 ----
    // 1) exe 内嵌图标资源（Explorer 文件管理器里的文件图标、Alt+Tab 大图标）：
    //    embed-resource 调 rc.exe 把 assets/icon.rc 编成 .res，以
    //    cargo:rustc-link-arg-bins 链进 mbvue.exe。
    //    .rc 里的 "assets/icon.ico" 相对路径基于 build script 的工作目录解析，
    //    而 cargo 把工作目录设为包根目录，与手动在项目根跑 rc.exe 等价（已实测）。
    //    rc.exe 缺失时只 warning 不失败——那只会少一个 Explorer 图标，
    //    任务栏/标题栏图标由 main.rs 的 WM_SETICON 负责，不受影响。
    if manifest.join("assets").join("icon.rc").exists() {
        if embed_resource::compile("assets/icon.rc", embed_resource::NONE)
            .manifest_optional()
            .is_ok()
        {
            println!("cargo:warning=已嵌入 Windows 图标资源 assets/icon.rc");
        } else {
            println!("cargo:warning=嵌入图标资源失败（未找到 rc.exe？仅缺 Explorer 文件图标）");
        }
        println!("cargo:rerun-if-changed={}", manifest.join("assets").join("icon.rc").display());
        println!("cargo:rerun-if-changed={}", manifest.join("assets").join("icon.ico").display());
    }

    // 2) 运行时窗口图标：把 assets/icon.ico 拷到 target/<profile>/，
    //    main.rs 启动后从 exe 同级目录 LoadImageW + WM_SETICON
    //    （任务栏按钮 / 标题栏左上角 / Alt+Tab）。与 DLL 部署同一机制。
    let icon_src = manifest.join("assets").join("icon.ico");
    if icon_src.exists() {
        let dst = target_dir.join("icon.ico");
        let same = dst.exists()
            && dst.metadata().map(|m| m.len()).unwrap_or(0)
                == icon_src.metadata().map(|m| m.len()).unwrap_or(1);
        if !same {
            if let Err(e) = std::fs::copy(&icon_src, &dst) {
                println!("cargo:warning=拷贝 icon.ico 失败: {}", e);
            } else {
                println!("cargo:warning=已部署 icon.ico -> {}", dst.display());
            }
        }
    }
}

/// 扫描 `build_dist/`（前端产物），生成 `$OUT_DIR/mb_assets.rs`：
///
/// ```rust
/// pub static MB_ASSETS: &[(&str, &[u8])] = &[("/index.html", b"..."), ...];
/// pub const MB_ASSETS_EMBEDDED: bool = true;
/// ```
///
/// server.rs 用 `include!` 它，release 版就直接从 exe 内部提供静态资源。
///
/// 为什么用 build 脚本生成而不用 `include_bytes!` 直接写死路径：
/// `include_bytes!` 的路径是**相对当前源文件**的，且要求文件在编译前就存在；
/// 放进 build.rs 里可以自己算绝对路径、缺文件时优雅降级（只 warning 不报错），
/// 这样第一次 `cargo build`（还没跑过 vite）也不会失败。
///
/// 注意：Vite 产物的文件名带内容哈希（`index-7ZymI7tF.js`），
/// 所以必须 `rerun-if-changed` 整个目录，否则改了前端不重新编译就是旧内容。
fn emit_asset_table(manifest: &Path, out_dir: &Path) {
    let dist = manifest.join("build_dist");
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    if dist.is_dir() {
        collect_files(&dist, &dist, &mut files);
    }
    files.sort();

    let gen = out_dir.join("mb_assets.rs");
    let Ok(mut f) = std::fs::File::create(&gen) else {
        println!("cargo:warning=无法创建 {}，前端资源将不内嵌", gen.display());
        return;
    };

    if files.is_empty() {
        // 没有产物（比如刚 clone 还没跑 vite）：生成空表，
        // server.rs 会回退到「读磁盘目录」，开发期照常工作。
        let _ = writeln!(
            f,
            "// build.rs 自动生成：未找到 build_dist/，前端资源不内嵌（将回退读磁盘）\npub static MB_ASSETS: &[(&str, &[u8])] = &[];\npub const MB_ASSETS_EMBEDDED: bool = false;\n"
        );
        println!("cargo:warning=未找到 build_dist/，前端资源不内嵌（开发期读磁盘）");
    } else {
        let _ = writeln!(
            f,
            "// build.rs 自动生成，勿手改。源目录: {}",
            dist.display()
        );
        let _ = writeln!(
            f,
            "pub static MB_ASSETS: &[(&str, &[u8])] = &["
        );
        let mut total = 0usize;
        for (rel, abs) in &files {
            let Ok(bytes) = std::fs::read(abs) else {
                continue;
            };
            total += bytes.len();
            // 路径统一转成 URL 形式：反斜杠→正斜杠、前导斜杠
            let url = format!("/{}", rel.replace('\\', "/"));
            let _ = writeln!(
                f,
                "    ({:?}, include_bytes!({:?}) as &[u8]),",
                url,
                abs.display().to_string()
            );
        }
        let _ = writeln!(f, "];");
        let _ = writeln!(f, "pub const MB_ASSETS_EMBEDDED: bool = true;");
        println!(
            "cargo:warning=已内嵌前端资源 {} 个文件 / {} 字节",
            files.len(),
            total
        );
    }

    // 让 Cargo 感知前端产物变化：整目录作为依赖（文件名带哈希，单列文件会漏）
    println!("cargo:rerun-if-changed={}", dist.display());
    // 某个文件被删也要触发重建，因此额外监视目录本身
    println!("cargo:rerun-if-changed={}", manifest.join("vite.config.ts").display());
}

/// 递归收集目录下的文件，返回 (相对路径, 绝对路径)
fn collect_files(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            collect_files(base, &p, out);
        } else if ft.is_file() {
            if let Ok(rel) = p.strip_prefix(base) {
                out.push((rel.to_string_lossy().to_string(), p));
            }
        }
    }
}
