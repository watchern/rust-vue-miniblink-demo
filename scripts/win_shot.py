"""纯标准库的 Win32 窗口截图（零依赖，不需要 Pillow）。

为什么不用 Pillow：验证脚本要能在任意 python 解释器下直接跑，
不能假设装了第三方包。这里直接走 GDI：PrintWindow 拿窗口客户区位图，
再自己编码成 PNG（zlib + CRC32 都是标准库）。

用法:
    from win_shot import capture_hwnd
    ok = capture_hwnd(hwnd, "out.png")

参考: MSDN PrintWindow / GetDIBits / BITMAPINFOHEADER。
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import struct
import zlib

u = ctypes.windll.user32
g = ctypes.windll.gdi32
k = ctypes.windll.kernel32

try:
    u.SetProcessDPIAware()
except Exception:
    pass

# GDI 句柄常量
SRCCOPY = 0x00CC0020
DIB_RGB_COLORS = 0
BI_RGB = 0
PW_RENDERFULLCONTENT = 0x00000002  # Win8.1+，抓硬件加速/合成内容必需


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wt.DWORD),
        ("biWidth", ctypes.c_long),
        ("biHeight", ctypes.c_long),
        ("biPlanes", wt.WORD),
        ("biBitCount", wt.WORD),
        ("biCompression", wt.DWORD),
        ("biSizeImage", wt.DWORD),
        ("biXPelsPerMeter", ctypes.c_long),
        ("biYPelsPerMeter", ctypes.c_long),
        ("biClrUsed", wt.DWORD),
        ("biClrImportant", wt.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wt.DWORD * 3)]


def window_rect(hwnd: int) -> tuple[int, int, int, int]:
    r = wt.RECT()
    if not u.GetWindowRect(wt.HWND(hwnd), ctypes.byref(r)):
        return (0, 0, 0, 0)
    return (r.left, r.top, r.right, r.bottom)


def client_rect_size(hwnd: int) -> tuple[int, int]:
    r = wt.RECT()
    if not u.GetClientRect(wt.HWND(hwnd), ctypes.byref(r)):
        return (0, 0)
    return (r.right - r.left, r.bottom - r.top)


def capture_hwnd(hwnd: int, out_path: str, client_only: bool = True) -> bool:
    """抓 hwnd 窗口并存成 PNG。返回是否成功。"""
    if not hwnd:
        return False
    x, y, x2, y2 = window_rect(hwnd)
    w_full, h_full = x2 - x, y2 - y
    if client_only:
        cw, ch = client_rect_size(hwnd)
    else:
        cw, ch = w_full, h_full
    if cw <= 0 or ch <= 0:
        return False

    hdc = u.GetDC(hwnd)
    if not hdc:
        return False
    mem_dc = g.CreateCompatibleDC(hdc)
    bmp = g.CreateCompatibleBitmap(hdc, cw, ch)
    old = None
    try:
        if not mem_dc or not bmp:
            return False
        old = g.SelectObject(mem_dc, bmp)
        # PW_RENDERFULLCONTENT 对 miniblink / 硬件加速窗口是必须的，
        # 否则抓到的是黑屏（Chromium 走 GPU 合成，普通 WM_PAINT 抓不到）。
        ok = u.PrintWindow(wt.HWND(hwnd), mem_dc, PW_RENDERFULLCONTENT)
        if not ok:
            ok = u.PrintWindow(wt.HWND(hwnd), mem_dc, 0)
        if not ok:
            # 最后兜底：BitBlt 屏幕区域（窗口被遮挡时可能截到别的内容）
            g.BitBlt(mem_dc, 0, 0, cw, ch, hdc, 0, 0, SRCCOPY)

        bi = BITMAPINFO()
        bi.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
        bi.bmiHeader.biWidth = cw
        bi.bmiHeader.biHeight = -ch  # 负数 = 自上而下，省一次翻转
        bi.bmiHeader.biPlanes = 1
        bi.bmiHeader.biBitCount = 32
        bi.bmiHeader.biCompression = BI_RGB

        buf = ctypes.create_string_buffer(cw * ch * 4)
        got = g.GetDIBits(mem_dc, bmp, 0, ch, buf, ctypes.byref(bi), DIB_RGB_COLORS)
        if not got:
            return False
        return write_bmp_as_png(buf.raw[: cw * ch * 4], cw, ch, out_path)
    finally:
        if old:
            g.SelectObject(mem_dc, old)
        g.DeleteObject(bmp)
        g.DeleteDC(mem_dc)
        u.ReleaseDC(hwnd, hdc)


def write_bmp_as_png(bgra: bytes, w: int, h: int, out_path: str) -> bool:
    """把 32 位 BGRA 自上而下数据编码成 PNG（纯标准库）。

    注意两处格式差异：
      1. GDI 内存里字节序是 B,G,R,A，PNG color type 6 期望 R,G,B,A → 需交换 R/B；
      2. GDI 32 位位图的 alpha 字节恒为 0，直接写进 PNG 会变成全透明 → 强制 255。
    交换与置 alpha 都用切片批量做（每行 2 次赋值），避免逐像素 Python 循环。
    """
    stride = w * 4
    opaque = b"\xff" * w
    raw = bytearray()
    for y in range(h):
        row = bytearray(bgra[y * stride : (y + 1) * stride])
        row[0::4], row[2::4] = row[2::4], row[0::4]  # B <-> R（每像素第 0/2 字节）
        row[3::4] = opaque
        raw.append(0)  # filter type 0 = None
        raw += row

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)  # 8bit RGBA
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
        + chunk(b"IEND", b"")
    )
    with open(out_path, "wb") as f:
        f.write(png)
    return True
