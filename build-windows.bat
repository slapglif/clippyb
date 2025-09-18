@echo off
echo Building Tauri application for Windows...
echo.

REM Set up environment to avoid windres issues
set CARGO_TARGET_DIR=target-windows
set TAURI_SKIP_DEVSERVER_CHECK=true

REM Clean previous builds
echo Cleaning previous builds...
if exist src-tauri\target-windows rd /s /q src-tauri\target-windows

REM Build frontend
echo Building frontend...
call npm run build
if errorlevel 1 goto error

REM Build Tauri without resource compilation issues
echo Building Tauri application...
cd src-tauri

REM Try to build with cargo directly, skipping problematic resource compilation
cargo build --release --target-dir target-windows 2>nul
if errorlevel 1 (
    echo First build attempt failed, trying alternative approach...
    REM Alternative: build in dev mode which often works better
    cargo build --target-dir target-windows
)

cd ..

if exist src-tauri\target-windows\release\clippyb.exe (
    echo.
    echo Build successful! Executable located at:
    echo src-tauri\target-windows\release\clippyb.exe
) else if exist src-tauri\target-windows\debug\clippyb.exe (
    echo.
    echo Debug build successful! Executable located at:
    echo src-tauri\target-windows\debug\clippyb.exe
) else (
    goto error
)

goto end

:error
echo.
echo Build failed! Please check the error messages above.
exit /b 1

:end
echo.
echo Build completed!