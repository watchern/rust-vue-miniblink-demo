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
!define APP_PROC      "mbvue"   ; 不带后缀的进程名（Get-Process -Name 用这个）
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
; 目录页「下一步」时检查目标位置是否已有实例在跑（该函数用最终选定的 $INSTDIR 判定）。
; MUI 的自定义回调 define 必须写在对应页面宏**之前**才生效。
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE DirPageLeave
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

; [升级安装：检测到目标位置已有实例在运行时的确认文案]
LangString MSG_RUNNING ${LANG_SIMPCHINESE} "检测到 ${APP_NAME_CN} 正在运行。$\r$\n$\r$\n安装需要先关闭它。是否立即关闭并继续安装？$\r$\n（选择「否」将留在当前页面，你可以更换安装位置或取消安装）"
LangString MSG_RUNNING ${LANG_ENGLISH} "${APP_NAME} is currently running.$\r$\n$\r$\nIt must be closed before installing. Close it now and continue?$\r$\n(Choosing No returns you to the previous page, where you can pick another location or cancel)"

; [升级安装：杀进程失败时的提示]
LangString MSG_STILL_RUNNING ${LANG_SIMPCHINESE} "仍有 ${APP_EXE} 进程未退出，安装无法继续。$\r$\n$\r$\n请手动关闭应用（右键托盘图标 → 退出）后重试。"
LangString MSG_STILL_RUNNING ${LANG_ENGLISH} "Some ${APP_EXE} processes could not be terminated, so the setup cannot continue.$\r$\n$\r$\nPlease close the app manually (right-click the tray icon and choose Exit), then try again."

; ---- 升级安装：目标位置已有实例在跑时的处理 ----
;
; 历史 bug（勿再回头）：PowerShell 命令原本直接写在 nsExec 的命令行里，会被**两层**吃掉：
;   ① NSIS 单引号字符串里写 '' —— 它被当成「字符串结束 + 新字符串开始」（相邻字符串会自动
;      拼接），单引号直接丢失，WQL 变成 `Name=mbvue.exe` → “无效查询”；
;   ② \" 在 NSIS 里**不是**转义（NSIS 的引号转义是 $\"），PowerShell 收到带反斜杠的引号后
;      直接报 TerminatorExpectedAtEndOfString。
; 于是检测永远返回「没在跑」，安装到一半撞上文件锁，弹出 NSIS 自带的
; 「无法打开要写入的文件 / 中止·重试·取消」—— 只有三个按钮且看不懂，体验很差。
;
; 正确做法（本文件现在的实现）：
;   · 把脚本写成 $TEMP 下的 .ps1，用 `powershell -File` 执行 —— 完全绕开命令行引号问题；
;   · 结果用**哨兵文件**传递（`.done` 表示跑完 / `.flag` 表示「在跑」），
;     不用 stdout 也不用退出码 —— 这样后台异步跑完也能取到结果；
;   · 按 $INSTDIR 精确匹配（不按映像名），不会误伤别处的便携版；
;   · 连子进程一起终止（miniblink 会派生渲染/GPU 子进程且它们同样持有 core.dll 句柄）。
;
; 体验上的关键设计：**检测提前到安装程序启动时就后台开跑**。
; 早期版本是在目录页点「下一步」时才同步跑 PowerShell（CIM 查询 1~3 秒），
; 用户会明显感到「点了没反应、卡一下才弹提示」。现在改成：
;   .onInit       → 后台异步启动检测（隐藏窗口，不阻塞任何界面）
;   DirPageLeave  → 直接取结果；目录被改过或后台还没跑完才退化为同步检测
; 于是绝大多数情况下（沿用注册表里那个安装位置）点「下一步」是**瞬时**响应。
Var IsRunning
Var PrefetchDir     ; 后台预检时用的 $INSTDIR，用来判断缓存结果是否还有效
Var KillOk          ; 0=进程已清空 / 1=仍有残留
Var v1

; 生成检测脚本。注意 NSIS 里 `$\"` 才是引号转义，`$$` 是字面 $。
Function WriteCheckScript
  FileOpen $0 "$TEMP\mbvue_check.ps1" w
  FileWriteUTF16LE /BOM $0 "$$runFlag = $\"$$env:TEMP\mbvue_running.flag$\"$\r$\n"
  FileWriteUTF16LE $0 "$$doneFlag = $\"$$env:TEMP\mbvue_running.done$\"$\r$\n"
  FileWriteUTF16LE $0 "Remove-Item $$runFlag,$$doneFlag -Force -ErrorAction SilentlyContinue$\r$\n"
  ; 快速通道：连同名进程都没有时立刻返回，省掉一次 CIM 全量枚举（全新安装走这条）
  FileWriteUTF16LE $0 "$$p = Get-Process -Name '${APP_PROC}' -ErrorAction SilentlyContinue$\r$\n"
  FileWriteUTF16LE $0 "if ($$p) {$\r$\n"
  FileWriteUTF16LE $0 "  $$t = Get-CimInstance Win32_Process -Filter $\"Name='${APP_EXE}'$\" | Where-Object { $$_.ExecutablePath -like '$INSTDIR*' }$\r$\n"
  FileWriteUTF16LE $0 "  if ($$t) { New-Item -ItemType File -Path $$runFlag -Force | Out-Null }$\r$\n"
  FileWriteUTF16LE $0 "}$\r$\n"
  FileWriteUTF16LE $0 "New-Item -ItemType File -Path $$doneFlag -Force | Out-Null$\r$\n"
  FileWriteUTF16LE $0 "exit 0$\r$\n"
  FileClose $0
FunctionEnd

; 同步跑一次检测（用于：静默安装、用户在目录页改了位置、后台预检没赶上）
Function RunCheckSync
  Call WriteCheckScript
  nsExec::ExecToStack 'powershell -NoProfile -ExecutionPolicy Bypass -File "$TEMP\mbvue_check.ps1"'
  Pop $0
  Call ReadCheckResult
FunctionEnd

; 从哨兵文件读结果 → $IsRunning
Function ReadCheckResult
  StrCpy $IsRunning "0"
  IfFileExists "$TEMP\mbvue_running.flag" setRunning done
  setRunning:
    StrCpy $IsRunning "1"
  done:
FunctionEnd

; 安装侧与卸载侧共用同一份实现。函数体写成宏再实例化两次，是因为
; NSIS 规定「卸载段里 Call 的函数名必须以 un. 开头」—— 否则编译期直接报
; `Call must be used with function names starting with "un."`。
; 「关掉 + 等它真的退出」放在同一次 PowerShell 调用里完成：
; 句柄释放有时延，以前是 NSIS 侧 Sleep 1.5s 再查一次，等于用户要多等两轮 PowerShell 启动；
; 现在脚本内部轮询（最多 8s），退出码 0=已清空 / 1=仍有残留。
!macro MK_KILL_RUNNING_APP prefix
  Function ${prefix}KillRunningApp
    FileOpen $0 "$TEMP\mbvue_kill.ps1" w
    FileWriteUTF16LE /BOM $0 "$$targets = Get-CimInstance Win32_Process -Filter $\"Name='${APP_EXE}'$\" | Where-Object { $$_.ExecutablePath -like '$INSTDIR*' }$\r$\n"
    FileWriteUTF16LE $0 "if (-not $$targets) { exit 0 }$\r$\n"
    FileWriteUTF16LE $0 "$$ids = $$targets | ForEach-Object { $$_.ProcessId }$\r$\n"
    FileWriteUTF16LE $0 "$$children = Get-CimInstance Win32_Process | Where-Object { $$ids -contains $$_.ParentProcessId }$\r$\n"
    FileWriteUTF16LE $0 "$$targets  | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force -ErrorAction SilentlyContinue }$\r$\n"
    FileWriteUTF16LE $0 "$$children | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force -ErrorAction SilentlyContinue }$\r$\n"
    FileWriteUTF16LE $0 "$$deadline = (Get-Date).AddSeconds(8)$\r$\n"
    FileWriteUTF16LE $0 "while ((Get-Date) -lt $$deadline) {$\r$\n"
    FileWriteUTF16LE $0 "  $$left = Get-CimInstance Win32_Process -Filter $\"Name='${APP_EXE}'$\" | Where-Object { $$_.ExecutablePath -like '$INSTDIR*' }$\r$\n"
    FileWriteUTF16LE $0 "  if (-not $$left) { exit 0 }$\r$\n"
    FileWriteUTF16LE $0 "  Start-Sleep -Milliseconds 300$\r$\n"
    FileWriteUTF16LE $0 "}$\r$\n"
    FileWriteUTF16LE $0 "exit 1$\r$\n"
    FileClose $0
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$TEMP\mbvue_kill.ps1"'
    Pop $0
    Delete "$TEMP\mbvue_kill.ps1"
    StrCpy $KillOk "0"
    ${If} $0 != "0"
      StrCpy $KillOk "1"
    ${EndIf}
  FunctionEnd
!macroend

!insertmacro MK_KILL_RUNNING_APP ""      ; → KillRunningApp（安装侧）
!insertmacro MK_KILL_RUNNING_APP "un."   ; → un.KillRunningApp（卸载侧）

; 目录页「下一步」：判定用的是用户**最终选定**的 $INSTDIR。
; 结果的三种来源，优先级从快到慢：
;   ① 目录没被改过 → 直接用启动时后台预检的哨兵文件（通常早就跑完了，瞬时）
;   ② 后台还没跑完（用户手速太快）→ 轮询等一下，最多 ~5s
;   ③ 目录被改过 / 后台异常 → 用新位置同步检测一次（退化路径，正确性优先）
Function DirPageLeave
  StrCmp $INSTDIR $PrefetchDir prefetch checkAgain

prefetch:
  StrCpy $v1 0
waitLoop:
  IfFileExists "$TEMP\mbvue_running.done" haveResult 0
  Sleep 200
  IntOp $v1 $v1 + 1
  IntCmp $v1 25 syncFallback waitLoop syncFallback   ; 25×200ms ≈ 5s
haveResult:
  Call ReadCheckResult
  Goto handle
syncFallback:
  Call RunCheckSync
  Goto handle
checkAgain:
  Call RunCheckSync

handle:
  ${If} $IsRunning == "1"
    MessageBox MB_YESNO|MB_ICONQUESTION "$(MSG_RUNNING)" IDYES killNow IDNO stayHere
    killNow:
      Call KillRunningApp
      ${If} $KillOk != "0"
        MessageBox MB_OK|MB_ICONEXCLAMATION "$(MSG_STILL_RUNNING)"
        Abort
      ${EndIf}
      Goto checked
    stayHere:
      Abort            ; 留在目录页，可更换位置或取消安装
    checked:
  ${EndIf}
FunctionEnd

Function .onInit
  ${IfNot} ${Silent}
    ; 交互安装：**一启动就后台开跑**检测。
    ; 位置刻意放在 MUI_LANGDLL_DISPLAY **之前**：语言选择框是模态的，放在它后面
    ; 的话预检会被用户「选语言」的时间堵住，白等。
    ; ExecShell 不等待、SW_HIDE 不弹控制台窗口，PowerShell 的启动开销因此与
    ; 用户选语言、读欢迎页、挑目录的时间完全重叠。
    Call WriteCheckScript
    StrCpy $PrefetchDir "$INSTDIR"
    Delete "$TEMP\mbvue_running.done"
    Delete "$TEMP\mbvue_running.flag"
    ExecShell "open" "powershell.exe" '-NoProfile -ExecutionPolicy Bypass -File "$TEMP\mbvue_check.ps1"' SW_HIDE
  ${EndIf}

  !insertmacro MUI_LANGDLL_DISPLAY

  ${If} ${Silent}
    ; 静默安装（/S）不走任何页面，DirPageLeave 不会触发，这里兜底：
    ; 无人可答，发现占用就直接关掉后继续。
    Call RunCheckSync
    ${If} $IsRunning == "1"
      Call KillRunningApp
    ${EndIf}
  ${EndIf}
FunctionEnd

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
  ; 与安装侧共用同一个 KillRunningApp：$TEMP 下写 .ps1 + powershell -File，
  ; 按 $INSTDIR 精确匹配（不按映像名，避免误杀用户另外在跑的便携版），并连子进程一起终止。
  ; （原先那两行是 wmic + 命令行 PowerShell —— 引号被 NSIS 两层吃掉，实际从未生效；
  ;   wmic 也已在 Win11 24H2 起被移除，统一走 PowerShell。）
  Call un.KillRunningApp
  Sleep 1500

  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\mb132_x64.dll"
  Delete "$INSTDIR\icon.ico"
  ; ---- 以下是内核运行时自己写出来的东西（设计上就落在 exe 同级，
  ;      装到 Program Files 不可写时会回退到 %LOCALAPPDATA%\mbvue，见下方清理）----
  ; cookies.dat：cookie 存储
  Delete "$INSTDIR\cookies.dat"
  ; config.json：窗口尺寸（window 段）及其它配置项
  Delete "$INSTDIR\config.json"
  ; 兼容旧版：记得窗口尺寸的 mbvue.window.json（扁平格式，已迁移到 config.json）
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
