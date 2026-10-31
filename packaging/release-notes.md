## 系统要求与运行库

所有 Linux 产物在 CI（Ubuntu 24.04 x64，glibc 2.39）上构建，二进制需要 **glibc ≥ 2.39**：
Ubuntu 24.04+ / Debian 13+ / Fedora 40+ / openSUSE Tumbleweed 或同代发行版。仅支持 x86_64。
更老的发行版会因 glibc 不足无法运行（可自行从源码构建，见项目 README）。

### Windows

| 产物 | 系统要求 | 运行库 |
|---|---|---|
| `*-windows-x64-setup.exe` | Windows 10 / 11 x64 | 无需额外安装——渲染内核 miniblink（Chromium 132）随包分发 |
| `*-windows-x64.zip` | Windows 10 / 11 x64 | 同上；解压即用（绿色版：`mbvue.exe` + `core.dll` + `icon.ico`） |

安装包支持升级安装（检测旧版进程，可选保留配置），卸载入口在系统"应用与功能"。

### Linux（WebKitGTK 4.1 壳，前端资源已内嵌）

| 产物 | 安装 / 运行方式 | 系统包依赖 |
|---|---|---|
| `.deb` | `sudo apt install ./mbvue-*-linux-x64.deb` | 自动解析 `libgtk-3-0`、`libwebkit2gtk-4.1-0`（Ubuntu 24.04+ / Debian 13+） |
| `.rpm` | Fedora: `sudo dnf install ./mbvue-*-linux-x64.rpm`<br>openSUSE: `sudo zypper install ./mbvue-*-linux-x64.rpm` | `gtk3`、`webkit2gtk4.1`（Fedora 40+） |
| `.AppImage` | `chmod +x mbvue-*.AppImage && ./mbvue-*.AppImage` | **自包含**：GTK3 / WebKitGTK 运行库与 WebKitWebProcess / WebKitNetworkProcess 助手已内置，系统无需安装 webkit（仅要求上文 glibc 版本） |
| `.tar.gz` | 解压后 `./mbvue` | 需系统已装：`libwebkit2gtk-4.1-0`、`libgtk-3-0`（Ubuntu: `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0`） |

安装版（deb/rpm）会注册桌面入口（`/usr/share/applications/mbvue.desktop`）与应用图标。

#### AppImage 备注

- 多数发行版开箱即用；无 FUSE2 的精简环境先执行 `./mbvue-*.AppImage --appimage-extract`，
  再运行 `squashfs-root/AppRun`
- AppImage 内关闭了 WebKit 沙箱（squashfs 挂载下 bwrap 不可用），这是 WebKitGTK 类
  AppImage 的通行做法（Tauri 同款）
- 应用只加载本地内嵌页面（http://127.0.0.1 随机端口），不需要 gio TLS 模块；如用
  `--url` 访问外网 https 站点，建议改用 deb/rpm 安装

### 通用说明

- 配置与日志：Windows 写 exe 同级目录（不可写时回退 `%LOCALAPPDATA%\mbvue`）；
  Linux 写 `~/.local/share/mbvue/`（`config.json` + `mbvue.log`）
- 运行不依赖外网与 Node/浏览器环境；`--dev` / `--url` / `--port` 等参数见 README
