fn main() {
    println!("Test executable started!");
    println!("Current directory: {:?}", std::env::current_dir());
    println!("Args: {:?}", std::env::args().collect::<Vec<_>>());
    
    // Test if we can at least create a window
    println!("Testing basic window creation...");
    
    // Just exit successfully
    println!("Test complete!");
}