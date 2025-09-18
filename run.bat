@echo off
echo Running ClippyB...

if exist "src-tauri\target\release\clippyb.exe" (
    echo Running release build...
    src-tauri\target\release\clippyb.exe
) else if exist "src-tauri\target\debug\clippyb.exe" (
    echo Running debug build...
    src-tauri\target\debug\clippyb.exe
) else (
    echo No build found! Building the app...
    cd src-tauri
    cargo build
    cd ..
    if exist "src-tauri\target\debug\clippyb.exe" (
        echo Build successful! Running...
        src-tauri\target\debug\clippyb.exe
    ) else (
        echo Build failed!
        pause
        exit /b 1
    )
)