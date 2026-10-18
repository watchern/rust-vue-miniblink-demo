"""安装包端到端验证。

做四件事（都可 --skip-* 关掉）：
  1. 静默安装到临时目录，核对安装后的文件清单与体积（验证「精简」真的生效）
  2. 启动装好的 exe，确认能渲染 + IPC 通（Rust 壳启动比 Electron 慢，多等）
  3. 截图存证
  4. 静默卸载，确认目录被清干净

注意（两个环境陷阱，踩过）：
  · 宿主 WorkBuddy 导出了 ELECTRON_RUN_AS_NODE，本项目是 Rust 壳不受影响，
    但 verify_resize.py 那套包装器的做法仍可借鉴：必须由 Python Popen 持住子进程，
    非交互 shell 退出会回收后台进程，表现为「页面已加载但窗口消失」。
  · 绝不能 taskkill /IM <name> 之外的宽泛规则误伤宿主；这里按窗口标题反查 PID 再杀。

用法：
  python scripts/verify_installer.py
  python scripts/verify_installer.py --skip-launch --skip-uninstall
"""
import argparse
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SETUP = ROOT / 'build_target' / 'mbvue-Setup.exe'
INSTALL_ROOT = Path(os.environ.get('TEMP', r'C:\Temp')) / 'mbvue-verify'
# 窗口标题不作为查找锚点：它会随界面语种变（`mbvue · <locale>`），
# 详见 kill_by_exe 的注释。这里只留作日志里的可读标识。
TITLE = 'miniblink + Vue 3 + Rust'
EXE_NAME = 'mbvue.exe'

u32 = ctypes.windll.user32
k32 = ctypes.windll.kernel32


def mb(n: int) -> str:
    return f'{n / 1048576:.2f} MB'


def find_window(title_sub: str | None, exe_sub: str | None = None):
    """枚举可见顶层窗口，可按标题子串和/或 exe 路径子串过滤。

    两个参数都可选、任一命中即收：
      · 只给 exe_sub → 按 exe 全路径匹配（**推荐**，见 kill_by_exe 的注释）
      · 只给 title_sub → 按标题子串匹配
      · 都给 → 两者都要满足
    """
    out = []

    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def cb(hwnd, _):
        if not u32.IsWindowVisible(hwnd):
            return True
        pid = wt.DWORD()
        u32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        path = proc_path(pid.value) if exe_sub else ''
        if exe_sub and exe_sub.lower() not in path.lower():
            return True
        n = u32.GetWindowTextLengthW(hwnd)
        title = ''
        if n:
            buf = ctypes.create_unicode_buffer(n + 1)
            u32.GetWindowTextW(hwnd, buf, n + 1)
            title = buf.value
        if title_sub and title_sub not in title:
            return True
        if not title and not exe_sub:
            return True
        out.append((hwnd, title, pid.value, path))
        return True

    u32.EnumWindows(cb, 0)
    return out


def proc_path(pid: int) -> str:
    h = k32.OpenProcess(0x1000, False, pid)
    if not h:
        return ''
    fn = k32.QueryFullProcessImageNameW
    fn.argtypes = [wt.HANDLE, wt.DWORD, wt.LPWSTR, ctypes.POINTER(wt.DWORD)]
    buf = ctypes.create_unicode_buffer(1024)
    size = wt.DWORD(1024)
    try:
        return buf.value if fn(h, 0, buf, ctypes.byref(size)) else ''
    except OSError:
        return ''
    finally:
        k32.CloseHandle(h)


def kill_by_exe(exe_path: str, rounds: int = 4) -> int:
    """按 exe 全路径反查 PID 并杀整棵进程树，返回杀掉的窗口数。

    **按 exe 路径反查 PID 并杀整棵进程树，返回杀掉的窗口数。**

    不要只按窗口标题反查：本应用的窗口标题会随界面语种变化
    （App.vue 的 watch 会把 document.title 设成 `mbvue · <locale>`），
    切成英文后标题变 `mbvue · en-US`，按旧标题 `miniblink + Vue 3 + Rust`
    查会一个都找不到 → 进程没被杀 → DLL 仍被占用 → 卸载删不掉（踩过）。
    exe 路径是不变量，用它当锚点才稳。
    """
    n = 0
    for _ in range(rounds):
        wins = find_window(None, exe_path)
        if not wins:
            break
        for w in wins:
            kill_tree(w[2])
            n += 1
        time.sleep(0.8)
    return n


def kill_tree(pid: int) -> None:
    """按 PID 杀整棵进程树。

    miniblink 内核会派生渲染 / GPU 子进程，它们同样持有 mb132_x64.dll 的句柄；
    只杀宿主 exe 的话 DLL 仍被占用，卸载就会删不掉。
    """
    subprocess.run(['taskkill', '/F', '/T', '/PID', str(pid)],
                   capture_output=True, encoding='utf-8', errors='replace')


def is_locked(path: Path) -> bool:
    """文件是否还被某个进程持有（打不开独占句柄 = 被锁）。

    不要用「文件是否消失」来判断进程是否杀干净——exe 是被**卸载器**删的，
    不是被 kill 删的，用它当判据会形成循环依赖：
    进程没杀干净 → 卸载删不掉 → 文件还在 → 误判「没杀干净」。
    直接问内核「能不能独占打开」才是准确信号。
    """
    GENERIC_READ = 0x80000000
    OPEN_EXISTING = 3
    FILE_SHARE_NONE = 0
    INVALID_HANDLE_VALUE = wt.HANDLE(-1).value
    h = k32.CreateFileW(str(path), GENERIC_READ, FILE_SHARE_NONE,
                        None, OPEN_EXISTING, 0x80, None)  # FILE_ATTRIBUTE_NORMAL
    if h == INVALID_HANDLE_VALUE or h is None:
        return True  # 打不开 = 被占用
    k32.CloseHandle(h)
    return False


def wait_unlocked(path: Path, timeout: float = 12.0) -> bool:
    """等文件锁被释放（进程退出后需要一点时间）。"""
    t0 = time.time()
    while time.time() - t0 < timeout:
        if not is_locked(path):
            return True
        time.sleep(0.4)
    return not is_locked(path)


def shot(hwnd: int, out_path: Path) -> bool:
    """抓窗口客户区并存 PNG（零依赖，不需 Pillow）。"""
    try:
        from win_shot import capture_hwnd
    except ImportError:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from win_shot import capture_hwnd
    try:
        return bool(capture_hwnd(hwnd, str(out_path), client_only=True))
    except Exception as e:
        print(f'    [warn] 截图异常: {e}')
        return False


def client_size(hwnd: int) -> tuple[int, int]:
    r = wt.RECT()
    if not u32.GetClientRect(hwnd, ctypes.byref(r)):
        return (0, 0)
    return (r.right - r.left, r.bottom - r.top)


def click_client(hwnd: int, x: int, y: int) -> bool:
    """在客户区 (x, y) 处发一次真实鼠标点击。

    用 PostMessage 而不是 SetCursorPos+mouse_event：后者会真的移动用户鼠标，
    在别人正在用机器时是干扰；而且本窗口是自定义无边框 + GPU 合成，
    PostMessage 的 WM_LBUTTONDOWN/UP 能正常被 miniblink 处理。
    """
    lp = (y << 16) | (x & 0xFFFF)
    u32.PostMessageW(wt.HWND(hwnd), 0x0201, 0x0001, lp)  # WM_LBUTTONDOWN
    u32.PostMessageW(wt.HWND(hwnd), 0x0202, 0x0000, lp)  # WM_LBUTTONUP
    return True


def click_lang_button(hwnd: int) -> bool:
    """点击标题栏的语种切换按钮。

    坐标从 TitleBar.vue 的样式推算（而非硬编码绝对值）：
      .titlebar 高 46、padding-right 4；.win-btns gap 2；
      .win-btn 宽 34；.lang-btn 宽 40 + margin-right 6；
      从右往左依次是 close / max / min / lang。
    """
    cw, ch = client_size(hwnd)
    if cw < 300 or ch < 100:
        return False
    # 从右往左：close(34) + gap2 + max(34) + gap2 + min(34) + gap2 + lang 的 margin-right(6) + lang(40)
    from_right = 4 + 34 + 2 + 34 + 2 + 34 + 2 + 6 + 40
    x = cw - from_right + 40 // 2  # 按钮中心
    y = 46 // 2
    return click_client(hwnd, x, y)


def listdir(p: Path):
    if not p.exists():
        return
    for f in sorted(p.rglob('*')):
        if f.is_file():
            print(f'    {str(f.relative_to(p)):46s} {mb(f.stat().st_size):>10s}')


def dir_size(p: Path) -> int:
    return sum(f.stat().st_size for f in p.rglob('*') if f.is_file()) if p.exists() else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--skip-install', action='store_true')
    ap.add_argument('--skip-launch', action='store_true')
    ap.add_argument('--skip-uninstall', action='store_true')
    ap.add_argument('--timeout', type=int, default=45, help='等窗口出现的秒数')
    ap.add_argument('--uninstall-timeout', type=int, default=40,
                    help='等卸载完成的秒数（UAC 提权有 3~5s 延迟，别设太小）')
    args = ap.parse_args()

    if not SETUP.exists():
        print(f'找不到安装包 {SETUP}，先跑 scripts/make_installer.sh')
        return 1

    ok = True
    print('=' * 62)
    print(f'安装包: {SETUP.name}  {mb(SETUP.stat().st_size)}')
    print('=' * 62)

    # ---------------------------------------------------------------- 1. 安装
    if not args.skip_install:
        print('\n[1] 静默安装到临时目录…')
        # 先清掉上一轮的残留（可能 >50 文件，用 shutil.rmtree 而不是逐个 unlink）
        if INSTALL_ROOT.exists():
            import shutil
            shutil.rmtree(INSTALL_ROOT, ignore_errors=True)
        r = subprocess.run([str(SETUP), '/S', f'/D={INSTALL_ROOT}'],
                           capture_output=True, encoding='utf-8', errors='replace', timeout=180)
        print(f'    安装器退出码 {r.returncode}')
        time.sleep(1.5)

        if not INSTALL_ROOT.exists():
            print('    FAIL: 安装目录未创建')
            return 1

        print('    安装后文件清单：')
        listdir(INSTALL_ROOT)
        total = dir_size(INSTALL_ROOT)
        n = len([f for f in INSTALL_ROOT.rglob('*') if f.is_file()])
        print(f'    共 {n} 个文件 / {mb(total)}')

        # 精简验证：不该出现的东西
        forbidden = ['mb132_x32.dll', '.pdb', 'mb132_x64.dll']
        bad = []
        for f in INSTALL_ROOT.rglob('*'):
            for pat in forbidden:
                if f.name.endswith(pat):
                    bad.append(f.name)
        # 新版前端已内嵌进 exe，不该再有 build_dist/ 或 resources/ 目录
        junk = [d.name for d in INSTALL_ROOT.iterdir()
                if d.is_dir() and d.name in ('deps', 'incremental', 'examples', 'src', 'src-rs',
                                             'vendor', 'build_dist', 'resources', 'LocalStorage')]
        if bad or junk:
            ok = False
            print(f'    FAIL: 精简未生效，多余文件/目录 {bad + junk}')
        else:
            print('    [OK] 精简生效：无 x32 dll / pdb / cargo 产物 / build_dist 目录')

        # 必需文件（前端已内嵌，所以只剩 4 个）
        must = [EXE_NAME, 'core.dll', 'icon.ico', 'uninstall.exe']
        missing = [m for m in must if not (INSTALL_ROOT / m).exists()]
        if missing:
            ok = False
            print(f'    FAIL: 缺少必需文件 {missing}')
        else:
            print(f'    [OK] 必需文件齐全（{len(must)} 项）')

    # ---------------------------------------------------------------- 2. 启动
    if not args.skip_launch:
        print('\n[2] 启动安装好的 exe…')
        exe = INSTALL_ROOT / EXE_NAME
        if not exe.exists():
            print(f'    FAIL: {exe} 不存在')
            return 1
        logf = open(ROOT / 'logs' / 'verify-installer.log', 'w', encoding='utf-8', errors='replace')
        env = dict(os.environ)
        env.pop('ELECTRON_RUN_AS_NODE', None)  # 习惯性清理，对 Rust 壳无影响
        proc = subprocess.Popen([str(exe)], cwd=str(INSTALL_ROOT),
                                stdout=logf, stderr=subprocess.STDOUT, env=env)
        try:
            wins = []
            t0 = time.time()
            while time.time() - t0 < args.timeout:
                if proc.poll() is not None:
                    print(f'    FAIL: 进程提前退出 code={proc.returncode}')
                    break
                wins = find_window(None, str(exe))
                if wins:
                    break
                time.sleep(0.5)
            if wins:
                hwnd, title, pid, path = wins[0]
                # 标题可能为空：miniblink 的窗口标题在页面加载完才由 JS 设上，
                # 匹配放宽到 exe 路径后可能先撞上还没设标题的那一刻。
                print(f'    [OK] 窗口已出现: {title!r}' if title
                      else '    [OK] 窗口已出现（标题尚未设置，属正常）')
                print(f'         pid={pid} exe={path}')
                time.sleep(3.0)  # 等前端渲染 + IPC 首屏

                # 截图存证：中文界面 + 切到英文后的界面，各一张。
                # 用 win_shot（纯 ctypes + zlib，不依赖 Pillow），且必须走
                # PrintWindow(PW_RENDERFULLCONTENT) —— miniblink 是 GPU 合成。
                shot_zh = ROOT / 'logs' / 'verify-installer-zh.png'
                shot_en = ROOT / 'logs' / 'verify-installer-en.png'
                if shot(hwnd, shot_zh):
                    print(f'    [OK] 截图(中文): {shot_zh.name}')
                    # 点标题栏语种按钮。Chromium 渲染进程里没有真实 Win32 按钮
                    # （不能靠 FindWindow+BM_CLICK），只能按客户区坐标点击。
                    if click_lang_button(hwnd):
                        time.sleep(2.0)  # 等 Vue 重渲染 + setLocale IPC 往返
                        if shot(hwnd, shot_en):
                            print(f'    [OK] 截图(英文): {shot_en.name}')
                        else:
                            print('    [warn] 英文截图失败')
                    else:
                        print('    [warn] 未能点击语种按钮（客户区尺寸异常）')
                else:
                    print('    [warn] 中文截图失败')
            else:
                ok = False
                print(f'    FAIL: {args.timeout}s 内未出现窗口')
        finally:
            # 杀进程顺序有讲究：
            #   1) 按 exe 全路径反查（标题会随语种变，路径不变）——能拿到内核起的
            #      渲染/GPU 子进程，它们同样持有 mb132_x64.dll 的句柄；
            #   2) 再按自己持有的 Popen pid 兜底；
            #   3) 最后等锁真正释放，否则卸载阶段删不掉文件。
            #      判据用「能否独占打开」而不是「文件是否消失」——exe 是卸载器删的，
            #      拿它当判据会形成循环依赖（详见 is_locked 的注释）。
            killed = kill_by_exe(str(exe))
            if proc.poll() is None:
                kill_tree(proc.pid)
            released = wait_unlocked(exe, timeout=15)
            print(f'    杀进程: exe 路径反查 {killed} 个 / Popen pid={proc.pid}'
                  f'，exe 锁{"已释放" if released else "未释放(!)"}')
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(5)
                except subprocess.TimeoutExpired:
                    proc.kill()
            logf.close()
            log = (ROOT / 'logs' / 'verify-installer.log')
            if log.exists() and log.stat().st_size:
                tail = log.read_text(encoding='utf-8', errors='replace').splitlines()[-8:]
                print('    --- 应用日志尾部 ---')
                for line in tail:
                    print('    ' + line)

    # ---------------------------------------------------------------- 3. 卸载
    if not args.skip_uninstall:
        print('\n[3] 静默卸载…')
        uninst = INSTALL_ROOT / 'uninstall.exe'
        if uninst.exists():
            r = subprocess.run([str(uninst), '/S'], capture_output=True,
                               encoding='utf-8', errors='replace', timeout=120)
            print(f'    卸载器退出码 {r.returncode}')
            # 必须轮询等待而不是固定 sleep：卸载器 RequestExecutionLevel admin，
            # 从非提权进程调用时会静默重提权，真正开始执行要等 3~5 秒。
            # 固定 sleep 2s 会在它还没开始删时就检查，必然误报一堆「残留」。
            t0 = time.time()
            left = []
            while time.time() - t0 < args.uninstall_timeout:
                left = ([str(f) for f in INSTALL_ROOT.rglob('*')]
                        if INSTALL_ROOT.exists() else [])
                # uninstall.exe 会在最后自删，属正常；只剩它（或已空）即视为完成
                left = [p for p in left if not p.endswith('uninstall.exe')]
                if not left:
                    break
                time.sleep(0.5)
            if left:
                ok = False
                print(f'    FAIL: 卸载后残留 {len(left)} 项: {left[:6]}')
            else:
                waited = time.time() - t0
                print(f'    [OK] 卸载干净（{waited:.1f}s 后确认，'
                      f'uninstall.exe 自删属正常）')
            import shutil
            shutil.rmtree(INSTALL_ROOT, ignore_errors=True)
        else:
            print('    [skip] 没有 uninstall.exe')

    print('\n' + '=' * 62)
    print('安装包验证全部通过' if ok else '存在失败项')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
