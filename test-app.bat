@echo off
echo Testing Tauri application...
echo.

cd src-tauri\target\release
start clippyb.exe

echo.
echo Application launched! Check if a window opened.
echo If the app started successfully, you should see:
echo - A tray icon appear
echo - Or a window (if visible is set to true)
echo.
pause