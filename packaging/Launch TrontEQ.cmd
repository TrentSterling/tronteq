@echo off
REM Launch TrontEQ. Uses the elevated autostart task, so there's no UAC prompt.
REM (TrontEQ also starts on its own every time you log in, once installed.)

schtasks /run /tn TrontEQ >nul 2>&1
if %errorlevel% neq 0 (
    echo TrontEQ isn't installed yet.
    echo Double-click "Install TrontEQ" first.
    echo.
    pause
)
