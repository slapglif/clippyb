// Test version without WebView2
use tauri::Manager;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("Starting test version...");
    
    // Create a simple file to prove the app is running
    std::fs::write("app_started.txt", "App started successfully").ok();
    
    println!("Test complete - check for app_started.txt");
}