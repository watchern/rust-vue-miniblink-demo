"""冷启动 / 内存实测脚本（三套桌面方案通用）。

用法:
    python measure.py <exe路径> <进程名> [--args "..."] [--keep] [--runs 3]

输出 JSON:
    {
      "proc": 进程名,
      "startupMs": 冷启动到窗口可见的毫秒数,
      "memMainMb": 主进程工作集,
      "memTreeMb": 主进程 + 全部子进程工作集合计（应用真实占用）,
      "pid": pid,
      "runs": [每次运行的明细]
    }

说明:
  - 冷启动定义：从创建进程到目标窗口首次可见
  - 内存：窗口显示 3 秒后读取
  - 默认测完杀掉进程；加 --keep 则留着（方便接着截图/交互）
  - 会自动剔除沙箱注入的 ELECTRON_RUN_AS_NODE / NODE_OPTIONS，避免污染测量
"""

import ctypes
import ctypes.wintypes as wt
import json
import os
import subprocess
import sys
import time

u = ctypes.windll.user32
k = ctypes.windll.kernel32
ps = ctypes.windll.psapi
u.SetProcessDPIAware()

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
TH32CS_SNAPPROCESS = 0x00000002


class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
    _fields_ = [
        ("cb", wt.DWORD),
        ("PageFaultCount", wt.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
    ]


class PROCESSENTRY32(ctypes.Structure):
    _fields_ = [
        ("dwSize", wt.DWORD),
        ("cntUsage", wt.DWORD),
        ("th32ProcessID", wt.DWORD),
        ("th32DefaultHeapID", ctypes.c_size_t),
        ("th32ModuleID", wt.DWORD),
        ("cntThreads", wt.DWORD),
        ("th32ParentProcessID", wt.DWORD),
        ("pcPriClassBase", ctypes.c_long),
        ("dwFlags", wt.DWORD),
        ("szExeFile", ctypes.c_wchar * 260),
    ]


def child_map():
    """返回 {ppid: [pid, ...]}"""
    snap = k.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    if snap == -1:
        return {}
    entry = PROCESSENTRY32()
    entry.dwSize = ctypes.sizeof(entry)
    out = {}
    ok = k.Process32FirstW(snap, ctypes.byref(entry))
    while ok:
        out.setdefault(entry.th32ParentProcessID, []).append(entry.th32ProcessID)
        ok = k.Process32NextW(snap, ctypes.byref(entry))
    k.CloseHandle(snap)
    return out


def descendants(root_pid):
    cmap = child_map()
    seen, stack = set(), [root_pid]
    while stack:
        cur = stack.pop()
        for ch in cmap.get(cur, []):
            if ch not in seen:
                seen.add(ch)
                stack.append(ch)
    return seen


def exe_of(pid):
    h = k.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not h:
        return ""
    try:
        buf = ctypes.create_unicode_buffer(512)
        size = wt.DWORD(512)
        if k.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(size)):
            return buf.value.rsplit("\\", 1)[-1].lower()
    finally:
        k.CloseHandle(h)
    return ""


def window_of(proc_name, root_pid=None, timeout=30.0, min_w=400, min_h=300):
    """轮询直到 proc_name 进程出现可见窗口，返回 hwnd。

    root_pid 非空时，只接受属于本次启动进程树（含子进程）的窗口，
    避免误匹配到同名的其他应用（比如 IDE 自己也是 Electron）。
    min_w/min_h 过滤掉崩溃时弹出的迷你对话框（如 Chromium 的 "Error" 框）。
    """
    target = proc_name.lower()
    deadline = time.time() + timeout
    allowed = None
    if root_pid:
        allowed = {root_pid} | descendants(root_pid)
    while time.time() < deadline:
        hits = []

        @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
        def cb(hwnd, _):
            if not u.IsWindowVisible(hwnd):
                return True
            rect = wt.RECT()
            if not u.GetWindowRect(hwnd, ctypes.byref(rect)):
                return True
            if (rect.right - rect.left) < min_w or (rect.bottom - rect.top) < min_h:
                return True
            pid = wt.DWORD()
            u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if exe_of(pid.value) != target:
                return True
            if allowed is not None and pid.value not in allowed:
                return True
            hits.append(hwnd)
            return True

        u.EnumWindows(cb, 0)
        if hits:
            return hits[0]
        time.sleep(0.05)
    return None


def mem_mb(pid):
    h = k.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x0400, False, pid)
    if not h:
        return None
    try:
        c = PROCESS_MEMORY_COUNTERS()
        c.cb = ctypes.sizeof(c)
        if ps.GetProcessMemoryInfo(h, ctypes.byref(c), c.cb):
            return round(c.WorkingSetSize / 1048576, 1)
    finally:
        k.CloseHandle(h)
    return None


def tree_mem(root_pid):
    total = 0.0
    for pid in [root_pid] + sorted(descendants(root_pid)):
        m = mem_mb(pid)
        if m:
            total += m
    return round(total, 1)


def clean_env():
    env = dict(os.environ)
    for bad in ("ELECTRON_RUN_AS_NODE", "NODE_OPTIONS"):
        env.pop(bad, None)
    return env


def main():
    exe = sys.argv[1]
    proc = sys.argv[2]
    extra, keep, runs, cwd = [], False, 1, None
    if "--args" in sys.argv:
        i = sys.argv.index("--args")
        extra = sys.argv[i + 1].split()
    if "--keep" in sys.argv:
        keep = True
    if "--runs" in sys.argv:
        i = sys.argv.index("--runs")
        runs = int(sys.argv[i + 1])
    if "--cwd" in sys.argv:
        i = sys.argv.index("--cwd")
        cwd = sys.argv[i + 1]
    if cwd is None:
        cwd = os.path.dirname(os.path.abspath(exe))

    details = []
    for i in range(runs):
        t0 = time.perf_counter()
        p = subprocess.Popen([exe] + extra, cwd=cwd, env=clean_env())
        hwnd = window_of(proc, root_pid=p.pid)
        startup_ms = round((time.perf_counter() - t0) * 1000, 1)
        if not hwnd:
            print(json.dumps({"proc": proc, "startupMs": None, "error": "窗口未出现"}))
            p.kill()
            return
        time.sleep(2.0)
        # 等子进程（渲染进程 / WebView2 / GPU）起来并稳定，最多再等 4 秒
        last, stable = -1, 0
        for _ in range(8):
            n = len(descendants(p.pid))
            stable = stable + 1 if n == last else 0
            if stable >= 2:
                break
            last = n
            time.sleep(0.5)
        rec = {
            "proc": proc,
            "startupMs": startup_ms,
            "memMainMb": mem_mb(p.pid),
            "memTreeMb": tree_mem(p.pid),
            "pid": p.pid,
        }
        details.append(rec)
        print(json.dumps(rec))
        if i == runs - 1 and keep:
            with open("_measure_pid.txt", "w") as f:
                f.write(str(p.pid))
        else:
            p.kill()
            time.sleep(1.5)

    ok = [d for d in details if d["startupMs"] is not None]
    if len(ok) > 1:
        summary = {
            "proc": proc,
            "startupMs": round(min(d["startupMs"] for d in ok), 1),
            "startupMsAvg": round(sum(d["startupMs"] for d in ok) / len(ok), 1),
            "memTreeMb": max(d["memTreeMb"] or 0 for d in ok),
            "runs": details,
        }
        print(json.dumps(summary))


if __name__ == "__main__":
    main()
