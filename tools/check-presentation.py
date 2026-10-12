"""Check an explicitly selected TrontEQ GUI, using only its own screenshot pipe.

Run after launching tronteq.exe --visualizer --present. Escape is sent directly
to the supplied process's verified TrontEQ window; no global input or focus change.
Use --gallery after launching tronteq.exe --gallery to check the scene gallery.
"""
import ctypes as c
from ctypes import wintypes as w
import pathlib
import sys
import time
import struct

pid = int(sys.argv[1])
gallery = "--gallery" in sys.argv[2:]
out = pathlib.Path(__file__).resolve().parents[1] / "target"
u = c.WinDLL("user32", use_last_error=True)
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.IsWindowVisible.argtypes = [w.HWND]
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.PostMessageW.restype = w.BOOL
windows = []
@c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
def enum(hwnd, _):
    owner = w.DWORD()
    u.GetWindowThreadProcessId(hwnd, c.byref(owner))
    name = c.create_unicode_buffer(128)
    u.GetWindowTextW(hwnd, name, 128)
    if owner.value == pid and name.value == "TrontEQ" and u.IsWindowVisible(hwnd):
        windows.append(hwnd)
    return True
deadline = time.monotonic() + 10
while time.monotonic() < deadline:
    windows.clear()
    u.EnumWindows(enum, 0)
    if windows:
        break
    time.sleep(0.1)
assert len(windows) == 1, "Expected one visible TrontEQ window for the supplied PID"
hwnd = windows[0]

def capture(name):
    request = pathlib.Path("C:/ProgramData/TrontEq/shot.req")
    request.write_text("capture")
    deadline = time.monotonic() + 8
    while request.exists() and time.monotonic() < deadline:
        time.sleep(0.1)
    assert not request.exists(), "TrontEQ screenshot request timed out"
    data = pathlib.Path("C:/ProgramData/TrontEq/shot.png").read_bytes()
    (out / name).write_bytes(data)
    print("CAPTURE", out / name, "size", struct.unpack(">II", data[16:24]))

time.sleep(4)  # allow the presentation hint to fade
capture("gallery-live-open.png" if gallery else "looks-live-presentation.png")
owner = w.DWORD()
u.GetWindowThreadProcessId(hwnd, c.byref(owner))
assert owner.value == pid and u.IsWindowVisible(hwnd), "Selected TrontEQ window is no longer available"
assert u.PostMessageW(hwnd, 0x100, 0x1B, 0x00010001), "Escape key-down failed"
assert u.PostMessageW(hwnd, 0x101, 0x1B, 0xC0010001), "Escape key-up failed"
time.sleep(2)
capture("gallery-live-return.png" if gallery else "looks-live-return.png")
