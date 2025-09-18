@echo off
echo Building and Running Simple ClippyB...
cd src-tauri

echo Creating minimal version...
echo fn main() { println!("ClippyB - Music Clipboard Monitor"); println!("The full app has compilation issues that need to be fixed."); println!("Press Ctrl+C to exit."); loop { std::thread::sleep(std::time::Duration::from_secs(1)); } } > src\main_minimal.rs

echo Building minimal version...
rustc src\main_minimal.rs -o ..\clippyb-minimal.exe

cd ..

if exist clippyb-minimal.exe (
    echo Running minimal ClippyB...
    clippyb-minimal.exe
) else (
    echo Build failed!
    pause
)