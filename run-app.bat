@echo off
echo ========================================
echo Clippyb Tauri Application Launcher
echo ========================================
echo.

REM Check for WebView2
echo Checking WebView2 Runtime...
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" >nul 2>&1
if %errorlevel% neq 0 (
    echo.
    echo [!] WebView2 Runtime not found!
    echo.
    echo Tauri applications require Microsoft Edge WebView2 Runtime.
    echo Would you like to download it? (Y/N)
    choice /C YN /M "Download WebView2"
    if errorlevel 2 goto skipwebview
    if errorlevel 1 (
        echo.
        echo Opening WebView2 download page...
        start https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download-section
        echo.
        echo Please install WebView2 and then run this script again.
        pause
        exit /b 1
    )
)

:skipwebview
echo.
echo Starting Clippyb...
cd /d "%~dp0src-tauri\target\release"

REM Run the app
clippyb.exe

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Application exited with error code: %errorlevel%
    echo.
    echo Possible issues:
    echo - Missing WebView2 Runtime (most common)
    echo - Missing Visual C++ Redistributables
    echo - Windows Defender/Antivirus blocking
    echo.
    echo Try running as Administrator if the issue persists.
)

echo.
pause