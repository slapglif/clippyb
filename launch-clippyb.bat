@echo off
echo Starting clippyb...

REM Check if WebView2 runtime is installed
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >nul 2>&1
if errorlevel 1 (
    echo WebView2 runtime not found! Installing...
    winget install Microsoft.EdgeWebView2Runtime --accept-package-agreements --accept-source-agreements
    if errorlevel 1 (
        echo Failed to install WebView2 runtime. Please install manually from:
        echo https://developer.microsoft.com/en-us/microsoft-edge/webview2/
        pause
        exit /b 1
    )
)

REM Set environment variables
set WEBVIEW2_USER_DATA_FOLDER=%TEMP%\clippyb-webview2

REM Launch the application
echo Launching clippyb...
cd /d "%~dp0"
start "" "src-tauri\target\x86_64-pc-windows-gnu\release\clippyb.exe"

echo clippyb launched!
timeout /t 2 /nobreak >nul