@echo off
rem TrontEQ — one-click launcher.
rem 1. Ensures the APO is installed on your default output device.
rem 2. Launches the GUI.
rem Safe to run any number of times. Install is idempotent.

setlocal
pushd "%~dp0"

if not exist "apo\build\TrontEqApo.dll" (
    echo APO DLL not built. Run apo\build.bat first.
    popd
    exit /b 1
)
if not exist "target\release\tronteq-cli.exe" (
    echo tronteq-cli not built. Run: cargo build --release --workspace
    popd
    exit /b 1
)
if not exist "target\release\tronteq.exe" (
    echo tronteq GUI not built. Run: cargo build --release --workspace
    popd
    exit /b 1
)

echo.
echo [1/2] Installing APO on default output device...
target\release\tronteq-cli.exe install
if errorlevel 1 (
    echo.
    echo Install failed. If this says "test-signing must be enabled",
    echo reboot first -- it was enabled in the admin setup but boots don't apply it.
    popd
    exit /b 1
)

echo.
echo [2/2] Launching TrontEQ GUI...
echo       (If you don't hear EQ applied immediately, pause and resume any audio.)
start "" target\release\tronteq.exe

popd
endlocal
