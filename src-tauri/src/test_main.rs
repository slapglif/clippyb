fn main() {
    println!("Starting Tauri application...");
    
    // Try to run the app with panic catching
    match std::panic::catch_unwind(|| {
        clippyb_lib::run()
    }) {
        Ok(_) => println!("Application exited normally"),
        Err(e) => {
            println!("Application panicked!");
            if let Some(s) = e.downcast_ref::<&str>() {
                println!("Panic message: {}", s);
            } else if let Some(s) = e.downcast_ref::<String>() {
                println!("Panic message: {}", s);
            } else {
                println!("Unknown panic type");
            }
        }
    }
}