"""找到 mbvue.exe 的窗口，置顶后截窗口区域图。

用法: python shot.py <输出.png>
"""
import ctypes
import ctypes.wintypes as wt
import sys
import time

from PIL import ImageGrab

u = ctypes.windll.user32
k = ctypes.windll.kernel32
u.SetProcessDPIAware()

TARGET = "mbvue.exe"


def pid_of(hwnd):
    pid = wt.DWORD()
    u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def exe_name(pid):
    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
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


def find_window():
    hits = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(hwnd, _):
        if u.IsWindowVisible(hwnd):
            name = exe_name(pid_of(hwnd))
            if name == TARGET:
                hits.append(hwnd)
        return True

    u.EnumWindows(cb, 0)
    return hits[0] if hits else None


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "shot.png"
    hwnd = find_window()
    if not hwnd:
        print("NO_WINDOW")
        sys.exit(1)

    u.ShowWindow(hwnd, 9)  # SW_RESTORE
    u.SetForegroundWindow(hwnd)
    u.BringWindowToTop(hwnd)
    time.sleep(1.5)

    rect = wt.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(rect))
    l, t, r, b = rect.left, rect.top, rect.right, rect.bottom
    print(f"rect=({l},{t},{r},{b})")
    im = ImageGrab.grab(bbox=(l, t, r, b), all_screens=True)
    im.save(out)
    print(f"SAVED {out} {im.size}")


if __name__ == "__main__":
    main()
