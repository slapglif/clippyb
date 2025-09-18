@echo off
echo ==========================================
echo         CLIPPYB LAUNCHER
echo ==========================================
echo.

cd /d "%~dp0"

echo Checking application files...
if not exist "src-tauri\target\x86_64-pc-windows-gnu\release\clippyb.exe" (
    echo ERROR: clippyb.exe not found!
    echo Please run: npm run tauri build
    pause
    exit /b 1
)

if not exist "src-tauri\target\x86_64-pc-windows-gnu\release\WebView2Loader.dll" (
    echo Copying WebView2Loader.dll...
    copy "src-tauri\target\x86_64-pc-windows-gnu\release\build\webview2-com-sys-*\out\x64\WebView2Loader.dll" "src-tauri\target\x86_64-pc-windows-gnu\release\" >nul 2>&1
)

echo.
echo Starting Clippyb...
echo.

cd src-tauri\target\x86_64-pc-windows-gnu\release

REM Try to run with output
echo Running: clippyb.exe
echo.

start "Clippyb" clippyb.exe

echo.
echo Application launched!
echo.
echo If the window doesn't appear:
echo 1. Check if WebView2 Runtime is installed
echo 2. Try running as Administrator
echo 3. Check Windows Event Viewer for errors
echo.

timeout /t 5 >nul