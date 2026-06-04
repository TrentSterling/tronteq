@echo off
setlocal
rem TrontEQ APO build script. Windows SDK 10.0.26100, MSVC 14.44 (VS2022 Community).

set VS_DEV_CMD="C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if not exist %VS_DEV_CMD% (
    echo ERROR: vcvars64.bat not found at %VS_DEV_CMD%
    exit /b 1
)

call %VS_DEV_CMD% >nul

pushd "%~dp0"
if not exist build mkdir build

set SRC=src\TrontEqApo.cpp src\Dll.cpp src\Biquad.cpp src\SharedState.cpp src\Dynamics.cpp
set OUT=build\TrontEqApo.dll
set DEF=src\TrontEqApo.def

cl.exe /nologo /W4 /EHsc /std:c++17 /O2 /LD /MT /DUNICODE /D_UNICODE /D_USRDLL /D_ATL_DLL_IMPL ^
    /Fo:build\ ^
    %SRC% ^
    /link ^
    /DEF:%DEF% ^
    /OUT:%OUT% ^
    /IMPLIB:build\TrontEqApo.lib ^
    ole32.lib oleaut32.lib propsys.lib advapi32.lib user32.lib ksuser.lib mfuuid.lib ^
    AudioBaseProcessingObjectV140.lib audioeng.lib audiomediatypecrt.lib

if errorlevel 1 (
    echo.
    echo BUILD FAILED
    popd
    exit /b 1
)

echo.
echo Built %OUT%
dumpbin.exe /exports %OUT% | findstr /R "DllCanUnloadNow DllGetClassObject DllRegisterServer DllUnregisterServer"
popd
endlocal
