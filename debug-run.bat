@echo off
echo Debug Run Script
echo ================
echo.

cd /d "%~dp0src-tauri\target\x86_64-pc-windows-gnu\release"

echo Current directory:
cd
echo.

echo Files in directory:
dir *.dll *.exe
echo.

echo Starting application...
clippyb.exe
echo.
echo Exit code: %errorlevel%

if %errorlevel% neq 0 (
    echo.
    echo Application failed with error code: %errorlevel%
    
    if %errorlevel% equ -1073741515 echo Missing DLL error
    if %errorlevel% equ -1073741511 echo Entry point not found
)

pause