@echo off
echo ========================================
echo Tauri Application Debug Test
echo ========================================
echo.

echo Checking for WebView2...
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" >nul 2>&1
if %errorlevel% neq 0 (
    echo [WARNING] WebView2 Runtime not found in registry
    echo You may need to install it from:
    echo https://developer.microsoft.com/en-us/microsoft-edge/webview2/
    echo.
) else (
    echo [OK] WebView2 Runtime detected
    echo.
)

echo Running application with console output...
cd src-tauri\target\release

echo.
echo Starting clippyb.exe...
clippyb.exe
echo.
echo Exit code: %errorlevel%

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Application failed to start properly
    echo Common issues:
    echo - Missing WebView2 Runtime
    echo - Missing Visual C++ Redistributables
    echo - Antivirus blocking the app
)

echo.
pause