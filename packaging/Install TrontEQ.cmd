@echo off
title Install TrontEQ
REM One-click TrontEQ installer -- just double-click this file.
REM
REM It self-elevates EXACTLY ONCE (no re-elevation loop): if this window isn't
REM already admin, it relaunches itself elevated (one UAC prompt) and exits. The
REM elevated copy passes the admin check and runs bootstrap.ps1.

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Requesting administrator rights...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

echo Installing TrontEQ ^(elevated^)...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0bootstrap.ps1"
echo.
echo ------------------------------------------------------------
echo  If it said REBOOT REQUIRED: reboot now. TrontEQ finishes
echo  installing and launches automatically after you log back in.
echo  (If it doesn't, just double-click "Install TrontEQ" again.)
echo ------------------------------------------------------------
echo.
pause
