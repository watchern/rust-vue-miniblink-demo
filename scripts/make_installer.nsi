; mbvue (miniblink + Vue 3 + Rust) 的 NSIS 安装包脚本
;
; 为什么自己写而不是用 electron-builder：本项目是 Rust 壳，没有 Node 打包链路。
; 直接调 makensis（本机借用 electron-builder 缓存里的 NSIS 3.0.4.1）。
;
; 三个设计要点：
;   1. 只分发运行必需的文件 —— 排除 mb132_x32.dll（55MB，x64 进程根本不会加载）、
;      pdb、deps/、源码。安装后目录只有 exe + x64 dll + build_dist/ + icon.ico。
;   2. 安装器界面只保留简体中文与 English 两种（MUI_LANGUAGE 只列这两项）。
;   3. 用 lzma 最高压缩 —— mb132_x64.dll 66MB 是压缩率的胜负手。
;
; 多语言的坑（读 MUI2 源码 + 语言文件后确认，勿再简写）：
;   · MUI_LANGUAGE 内部会 LoadLanguageFile，同一个 .nlf 只能加载一次。
;     所以「每种语言声明一次，全部放最前面」之后，**不能再**用
;     `!insertmacro MUI_LANGUAGE "English"` 来切语言上下文 —— 那会二次加载
;     同一 nlf，编译期报 "can't load same language file twice"。
;   · 正确做法：用 LangString <键名> ${LANG_XXX} "…" 给同一个键在两种语言下
;     分别赋值。MUI 页面默认文案就取自这些 MUI_TEXT_* 键，所以覆盖它们即可实现
;     真正的双语界面，而无需重写页面逻辑。

Unicode true
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LangFile.nsh"

!define APP_NAME      "miniblink Vue Rust Demo"
!define APP_NAME_CN   "miniblink + Vue 3 + Rust 演示"
!define APP_PUBLISHER "workbuddy-demo"
!define APP_EXE       "mbvue.exe"
!define APP_VERSION   "0.1.0"
!define APP_REGKEY    "Software\workbuddy-demo\mbvue"

; ---- 源码目录（由 make_installer.sh 以 -D 传入，这里给默认值便于手工跑）----
!ifndef SRC_RELEASE
  !define SRC_RELEASE "..\build_target\release"
!endif
!ifndef SRC_DIST
  !define SRC_DIST "..\build_dist"
!endif
!ifndef SRC_ASSETS
  !define SRC_ASSETS "..\assets"
!endif
!ifndef OUT_DIR
  !define OUT_DIR "..\build_target"
!endif

Name        "${APP_NAME}"
OutFile     "${OUT_DIR}\mbvue-Setup.exe"
InstallDir  "$PROGRAMFILES64\mbvue-demo"
InstallDirRegKey HKLM "${APP_REGKEY}" "InstallDir"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${APP_VERSION}.0"
; LegalCopyright 是 VS 版资源里的标准键，缺了编译期会提示 9100
VIAddVersionKey /LANG=2052 "LegalCopyright" "Copyright (C) 2026 ${APP_PUBLISHER}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Copyright (C) 2026 ${APP_PUBLISHER}"
VIAddVersionKey /LANG=2052 "ProductName"     "${APP_NAME_CN}"
VIAddVersionKey /LANG=2052 "FileDescription" "${APP_NAME_CN} 安装程序"
VIAddVersionKey /LANG=2052 "FileVersion"     "${APP_VERSION}"
VIAddVersionKey /LANG=2052 "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey /LANG=2052 "CompanyName"     "${APP_PUBLISHER}"
VIAddVersionKey /LANG=1033 "ProductName"     "${APP_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${APP_NAME} Setup"
VIAddVersionKey /LANG=1033 "FileVersion"     "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "CompanyName"     "${APP_PUBLISHER}"

!define MUI_ABORTWARNING
; MUI2.nsh 已给这些宏设了默认值，必须先 undef 再 define，否则报 "already defined"。
; MUI_UNMUI_UNICON / MUI_FINISHPAGE_RUN_TEXT 在本 NSIS 版本里没有预定义，
; 对它们 undef 会报 6155，故加 ifdef 守卫。
!ifdef MUI_ICON
  !undef MUI_ICON
!endif
!ifdef MUI_UNICON
  !undef MUI_UNICON
!endif
!ifdef MUI_UNMUI_UNICON
  !undef MUI_UNMUI_UNICON
!endif
!ifdef MUI_FINISHPAGE_RUN_TEXT
  !undef MUI_FINISHPAGE_RUN_TEXT
!endif
!define MUI_ICON         "${SRC_ASSETS}\icon.ico"
!define MUI_UNICON       "${SRC_ASSETS}\icon.ico"
!define MUI_UNMUI_UNICON "${SRC_ASSETS}\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"

; 注册表记忆所选语言（只在这两种之间记）
!define MUI_LANGDLL_REGISTRY_ROOT "HKLM"
!define MUI_LANGDLL_REGISTRY_KEY  "Software\workbuddy-demo\mbvue"
!define MUI_LANGDLL_REGISTRY_VALUENAME "InstallerLanguage"
!define MUI_LANGDLL_ALWAYSSHOW

; ---- ① 页面宏：必须在 MUI_LANGUAGE 之前 ----
; MUI_PAGE_INIT 里 `!ifdef MUI_INSERT → warning`，而 MUI_INSERT 是在第一次
; MUI_LANGUAGE 时才定义的。所以官方推荐顺序是「页面 → 语言」，反了会一直告警。
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; ---- ② 两种语言各声明一次，且每种只能声明一次 ----
; MUI_LANGUAGE 内部会 LoadLanguageFile，同一个 .nlf 加载两次会报
; "can't load same language file twice"。要按语言给不同文案请用下面的 LangString。
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

; ---- ③ 文案覆盖（只补 MUI 语言文件里没有的，或确实想改的）----
; 重要：MUI2 自带的 SimpChinese.nsh / English.nsh 已经把目录页、安装进度、
; 完成页、卸载确认、中止警告等文案翻译好了。重复给这些键赋 LangString 会报
; "6030 set multiple times, wasting space" —— 那是覆盖，不是缺失。
; 所以这里只写两类：① 确实要改的欢迎页正文  ② 语言文件里没有的键。

; --- 欢迎页（想改成项目自己的介绍，所以覆盖）---
; [覆盖 MUI 自带译文]
LangString MUI_TEXT_WELCOME_INFO_TITLE ${LANG_SIMPCHINESE} "欢迎使用 ${APP_NAME_CN} 安装向导"

; [覆盖 MUI 自带译文]
LangString MUI_TEXT_WELCOME_INFO_TITLE ${LANG_ENGLISH} "Welcome to the ${APP_NAME} Setup Wizard"

; [覆盖 MUI 自带译文]
LangString MUI_TEXT_WELCOME_INFO_TEXT ${LANG_SIMPCHINESE} "miniblink(Chromium 132) + Vue 3 + Rust 桌面应用骨架。$\r$\n$\r$\n内核是自包含的单 DLL（无需外置资源），无边框窗口与本地 HTTP 服务全部由 Rust 壳驱动。$\r$\n$\r$\n$_CLICK"

; [覆盖 MUI 自带译文]
LangString MUI_TEXT_WELCOME_INFO_TEXT ${LANG_ENGLISH} "A desktop app skeleton built on miniblink (Chromium 132) + Vue 3 + Rust.$\r$\n$\r$\nThe kernel is a self-contained DLL (no external resources required); the borderless window and the local HTTP server are driven by the Rust shell.$\r$\n$\r$\n$_CLICK"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_TEXT_INSTFILES_RUNNING ${LANG_SIMPCHINESE} "正在安装…"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_TEXT_INSTFILES_RUNNING ${LANG_ENGLISH} "Installing…"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_FINISHPAGE_RUN_TEXT ${LANG_SIMPCHINESE} "启动 ${APP_NAME_CN}"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_FINISHPAGE_RUN_TEXT ${LANG_ENGLISH} "Launch ${APP_NAME}"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_LANGDLL_WINDOWTITLE ${LANG_SIMPCHINESE} "安装程序语言"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_LANGDLL_WINDOWTITLE ${LANG_ENGLISH} "Installer Language"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_LANGDLL_INFO ${LANG_SIMPCHINESE} "请选择安装界面语言。"

; [语言文件未定义，NSIS 核心默认英文]
LangString MUI_LANGDLL_INFO ${LANG_ENGLISH} "Please select the language for the installer."

Section "mbvue" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; ---- 全部内容就 3 个文件 ----
  File "${SRC_RELEASE}\${APP_EXE}"
  ; 唯一的内核 DLL。x64 进程在 Windows 上无法加载 x32 DLL，
  ; 所以 mb132_x32.dll（55MB）确定性地不进包。
  ;
  ; 这里装成 **core.dll**（源文件仍叫 mb132_x64.dll）：安装目录里
  ; "mb132_x64.dll" 这种内核内部版本号对终端用户没有意义，
  ; 而 core.dll 更贴合"内核"语义、也便于将来换内核版本时保持文件名稳定。
  ; main.rs 的 find_dll() 同时认 core.dll 与 mb132_x64.dll，改名不影响启动。
  ; 注意改名语法：NSIS 的 File 是 `File "src" /oname=out` 或 `File /oname=out "src"`，
  ; 不是 `File "src" "out"`（后者会被当成第二个待打包文件 → no files found）。
  File "/oname=core.dll" "${SRC_RELEASE}\mb132_x64.dll"
  ; 运行时窗口图标（exe 内嵌的图标只管资源管理器，任务栏/Alt+Tab 读这个文件）
  File "${SRC_ASSETS}\icon.ico"

  ; ---- 前端静态资源：不装 ----
  ; 已在编译期内嵌进 mbvue.exe（build.rs 生成资源表 + include_bytes!），
  ; 所以这里不需要 build_dist/ 目录。安装目录最终只有
  ; mbvue.exe + core.dll + icon.ico + uninstall.exe 四个文件。

  WriteRegStr HKLM "${APP_REGKEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${APP_REGKEY}" "Version" "${APP_VERSION}"

  ; 快捷方式：开始菜单 + 桌面
  CreateDirectory "$SMPROGRAMS\mbvue Demo"
  CreateShortcut "$SMPROGRAMS\mbvue Demo\mbvue Demo.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$SMPROGRAMS\mbvue Demo\Uninstall.lnk" "$INSTDIR\uninstall.exe"
  CreateShortcut "$DESKTOP\mbvue Demo.lnk" "$INSTDIR\${APP_EXE}"

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  ; 先关掉正在运行的实例，否则 exe 与内核 DLL 被占用删不掉。
  ; 用「按安装路径精确匹配」而不是 `taskkill /IM mbvue.exe`：/IM 按映像名匹配，
  ; 会把用户另外在跑的便携版（build_target\install\mbvue.exe）一起杀掉，
  ; 那是用户的东西，卸载器不该替用户做这个决定。
  ;
  ; 两道保险：
  ;   ① wmic —— 快，但 Win11 24H2 起已废弃，可能不存在；
  ;   ② PowerShell + CIM —— 慢但通用，兜底。
  ; 两条都刻意写成不用 $_（NSIS 会把 $_ 当成自己的变量而报 warning 6000），
  ; 改用 Invoke-CimMethod -MethodName Terminate 直接按对象终止。
  ;
  ; /T 的等价能力由 Terminate 覆盖不到子进程，这里靠 kernel32 层面的
  ; 进程树由 PowerShell 补：先杀父，miniblink 派生的渲染/GPU 子进程会随之退出。
  nsExec::ExecToLog 'cmd /c for /f "tokens=2 delims=," %%p in \'wmic process where "name=''${APP_EXE}'' and ExecutablePath like ''$INSTDIR%''" get processid /value\' do taskkill /F /T /PID %%p'
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "$$p = Get-CimInstance Win32_Process -Filter \"Name=''${APP_EXE}''\" | Where-Object ExecutablePath -like ''$INSTDIR*''; if ($$p) { $$p | ForEach-Object { Invoke-CimMethod -InputObject $$_ -MethodName Terminate | Out-Null } }"'

  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\mb132_x64.dll"
  Delete "$INSTDIR\icon.ico"
  ; ---- 以下是内核运行时自己写出来的东西（设计上就落在 exe 同级，
  ;      装到 Program Files 不可写时会回退到 %LOCALAPPDATA%\mbvue，见下方清理）----
  ; cookies.dat：cookie 存储
  Delete "$INSTDIR\cookies.dat"
  ; mbvue.window.json：记住的窗口尺寸/位置
  Delete "$INSTDIR\mbvue.window.json"
  ; LocalStorage\：DOM storage 落盘目录（界面语种偏好等）
  RMDir /r "$INSTDIR\LocalStorage"
  ; ---------------------------------------------------------------------------
  Delete "$INSTDIR\uninstall.exe"

  ; ---- 兼容清理：从旧版本升级上来时残留的东西 ----
  ; 新版装的是 core.dll（旧版是 mb132_x64.dll），两个都删。
  Delete "$INSTDIR\core.dll"
  Delete "$INSTDIR\mb132_x64.dll"
  Delete "$INSTDIR\mb132_x32.dll"
  ; 旧版安装过 build_dist\，新版前端已内嵌进 exe，目录要清掉。
  Delete "$INSTDIR\build_dist\index.html"
  RMDir /r "$INSTDIR\build_dist\assets"
  RMDir /r "$INSTDIR\build_dist"
  ; 兜底：目录里可能还有内核写的临时文件。
  ; 注意 RMDir /r 在文件被占用时会静默失败，所以卸载前必须先杀掉整棵进程树，
  ; 否则 core.dll 的句柄还在、这里删不动（验证脚本已按 exe 路径反查杀进程）。
  RMDir /r "$INSTDIR"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\mbvue Demo\mbvue Demo.lnk"
  Delete "$SMPROGRAMS\mbvue Demo\Uninstall.lnk"
  RMDir "$SMPROGRAMS\mbvue Demo"
  Delete "$DESKTOP\mbvue Demo.lnk"

  DeleteRegKey HKLM "${APP_REGKEY}"

  ; ---- 用户数据兜底清理 ----
  ; 正常情况运行时数据就在 $INSTDIR（上面已删）；但若装在 Program Files 且未提权、
  ; exe 同级不可写，应用会回退写到 %LOCALAPPDATA%\mbvue，这里一并清掉。
  ; 另外 Win7+ 非提权进程写 Program Files 可能被 UAC 重定向到
  ; %LOCALAPPDATA%\VirtualStore\Program Files\mbvue-demo\，那种情况也清掉。
  RMDir /r "$LOCALAPPDATA\mbvue"
  RMDir /r "$LOCALAPPDATA\VirtualStore\Program Files\mbvue-demo"
  RMDir /r "$LOCALAPPDATA\VirtualStore\Program Files (x86)\mbvue-demo"
  DetailPrint "运行期数据已随安装目录一并清理（含 %LOCALAPPDATA% 兜底位置）"
SectionEnd
