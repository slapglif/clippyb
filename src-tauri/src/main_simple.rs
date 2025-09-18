use clipboard_win::{formats, get_clipboard};
use std::thread;
use std::time::Duration;

fn main() {
    println!("🎵 ClippyB - Music Clipboard Monitor Started!");
    println!("Copy a song name, YouTube URL, or Spotify URL to download it.");
    println!("Press Ctrl+C to exit.\n");

    let mut last_clipboard = String::new();

    loop {
        // Get current clipboard content
        if let Ok(clipboard_content) = get_clipboard::<String, _>(formats::Unicode) {
            // Check if clipboard changed
            if clipboard_content != last_clipboard && !clipboard_content.is_empty() {
                last_clipboard = clipboard_content.clone();
                
                println!("📋 New clipboard content detected: {}", clipboard_content);
                
                // Check what type of content it is
                if clipboard_content.contains("youtube.com") || clipboard_content.contains("youtu.be") {
                    println!("🎥 YouTube URL detected!");
                } else if clipboard_content.contains("spotify.com") {
                    println!("🎵 Spotify URL detected!");
                } else {
                    println!("🔍 Song name detected!");
                }
                
                println!("⏳ Would download: {}\n", clipboard_content);
            }
        }
        
        // Sleep for 500ms before checking again
        thread::sleep(Duration::from_millis(500));
    }
}