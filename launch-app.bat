@echo off
echo Starting Clippyb Application...
echo.

cd /d "%~dp0"
start "" "src-tauri\target\release\clippyb.exe"

echo.
echo Application launched!
echo Check your system tray or taskbar for the window.
echo.
timeout /t 3 >nul