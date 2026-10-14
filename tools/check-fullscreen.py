"""Exercise F11, scene navigation, Escape and F11 exit on one verified live GUI.

Only sends window messages to the supplied TrontEQ process. Uses the app's
self-capture pipe; never changes global focus, pointer position or keyboard input.
"""
import ctypes as c
from ctypes import wintypes as w
import json
import pathlib
import sys
import time

process_id = int(sys.argv[1])
out = pathlib.Path(__file__).resolve().parents[1] / "target" / "fullscreen-verification"
out.mkdir(exist_ok=True)
data = pathlib.Path("C:/ProgramData/TrontEq")
u = c.WinDLL("user32", use_last_error=True)
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.IsWindowVisible.argtypes = [w.HWND]
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.PostMessageW.restype = w.BOOL
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.MonitorFromWindow.argtypes = [w.HWND, w.DWORD]
u.MonitorFromWindow.restype = w.HANDLE
windows = []

@c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
def enum(hwnd, _):
    owner = w.DWORD()
    u.GetWindowThreadProcessId(hwnd, c.byref(owner))
    name = c.create_unicode_buffer(128)
    u.GetWindowTextW(hwnd, name, 128)
    if owner.value == process_id and name.value == "TrontEQ" and u.IsWindowVisible(hwnd):
        windows.append(hwnd)
    return True

deadline = time.monotonic() + 12
while time.monotonic() < deadline:
    windows.clear()
    u.EnumWindows(enum, 0)
    if windows:
        break
    time.sleep(0.1)
assert len(windows) == 1, "Expected one visible TrontEQ window"
hwnd = windows[0]

def rect():
    bounds = w.RECT()
    assert u.GetWindowRect(hwnd, c.byref(bounds))
    return [bounds.left, bounds.top, bounds.right, bounds.bottom]

def key(code):
    owner = w.DWORD()
    u.GetWindowThreadProcessId(hwnd, c.byref(owner))
    assert owner.value == process_id and u.IsWindowVisible(hwnd)
    assert u.PostMessageW(hwnd, 0x100, code, 0x00010001)
    assert u.PostMessageW(hwnd, 0x101, code, 0xC0010001)

def capture(name):
    request = data / "shot.req"
    request.write_text("capture", encoding="ascii")
    deadline = time.monotonic() + 8
    while request.exists() and time.monotonic() < deadline:
        time.sleep(0.1)
    assert not request.exists(), "Screenshot timed out"
    (out / (name + ".png")).write_bytes((data / "shot.png").read_bytes())

def settings():
    return json.loads((data / "settings.json").read_text(encoding="utf-8-sig"))

class Monitor(c.Structure):
    _fields_ = [("size", w.DWORD), ("monitor", w.RECT), ("work", w.RECT), ("flags", w.DWORD)]

u.GetMonitorInfoW.argtypes = [w.HANDLE, c.POINTER(Monitor)]
monitor = Monitor()
monitor.size = c.sizeof(Monitor)
assert u.GetMonitorInfoW(u.MonitorFromWindow(hwnd, 2), c.byref(monitor))
m = monitor.monitor
monitor_bounds = [m.left, m.top, m.right, m.bottom]
time.sleep(1)
# A visible native window can precede shader compilation and the first settings
# migration. The self-capture response confirms a complete application frame.
capture("before")
before = settings()
(out / "settings-before.json").write_text(json.dumps(before, indent=2), encoding="utf-8")
sound = (data / "state.bin").read_bytes()[8:200]
initial_rect = rect()
entered = False
try:
    key(0x7A)  # F11
    entered = True
    time.sleep(4)
    assert rect() == monitor_bounds, f"F11 did not fill the monitor: {rect()} vs {monitor_bounds}"
    capture("f11-idle")
    key(0x27)  # Right
    time.sleep(1)
    assert settings()["show_mode"] != before["show_mode"], "Right arrow did not advance"
    key(0x25)  # Left
    time.sleep(1)
    assert settings()["show_mode"] == before["show_mode"], "Left arrow did not restore scene"
    key(0x1B)
    entered = False
    time.sleep(1)
    assert rect() == initial_rect, f"Escape did not restore window: {rect()} vs {initial_rect}"
    capture("escape-return")
    key(0x7A)
    entered = True
    time.sleep(1)
    assert rect() == monitor_bounds
    key(0x7A)
    entered = False
    time.sleep(1)
    assert rect() == initial_rect, "Second F11 did not restore window"
finally:
    if entered:
        key(0x1B)

after = settings()
(out / "settings-after.json").write_text(json.dumps(after, indent=2), encoding="utf-8")
changed = {k: {"before": before.get(k), "after": after.get(k)}
           for k in before.keys() | after.keys() if before.get(k) != after.get(k)}
assert not changed, "Presentation changed persisted preferences: " + json.dumps(changed)
assert (data / "state.bin").read_bytes()[8:200] == sound, "Sound controls changed during check"
receipt = dict(process_id=process_id, initial_rect=initial_rect, fullscreen_rect=monitor_bounds,
               f11=True, escape=True, repeat_f11=True, scene_navigation=True,
               preferences_preserved=True, sound_controls_preserved=True)
(out / "result.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
print("FULLSCREEN PASS: F11 / Escape / F11 exit / scene navigation / preferences / sound controls")
print(out)
