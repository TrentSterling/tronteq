@echo off
REM Launch (or resurface) TrontEQ. Idempotent on purpose.
REM
REM Why this isn't just "schtasks /run": if TrontEQ is already alive but hidden
REM to tray -- and its tray icon has gone stale, which happens whenever the shell
REM drops it -- the task reports Running and `schtasks /run` REFUSES to do
REM anything at all. You click the shortcut, nothing happens, the app looks lost.
REM
REM Launching the exe directly always wins: a second instance hands its request
REM to the running one over the loopback port and exits, and the running one
REM raises its window. If nothing is running yet, that same launch just starts it.

if exist "%~dp0tronteq.exe" (
    start "" "%~dp0tronteq.exe"
    goto :eof
)

REM Fallback for layouts where the exe isn't next to this script: drive the
REM autostart task, then poke it a second time in case it was already Running.
schtasks /run /tn TrontEQ >nul 2>&1
if %errorlevel% neq 0 (
    echo TrontEQ isn't installed yet.
    echo Double-click "Install TrontEQ" first.
    echo.
    pause
)
