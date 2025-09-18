fn main() {
    println!("Simple test - executable can run!");
    println!("Rust version: {}", env!("RUSTC_VERSION"));
    println!("Target: {}", env!("TARGET"));
    println!("Current directory: {:?}", std::env::current_dir());
    
    // Test if we can at least load the Tauri context
    match std::panic::catch_unwind(|| {
        let _context = tauri::generate_context!();
        println!("Tauri context generated successfully");
    }) {
        Ok(_) => println!("Context test passed"),
        Err(e) => println!("Context test failed: {:?}", e),
    }
}