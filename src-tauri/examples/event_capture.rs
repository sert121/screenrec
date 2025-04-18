use rdev::{listen, Event};
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    // Get the output file path from command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <output_file>", args[0]);
        std::process::exit(1);
    }
    
    let output_file = &args[1];
    println!("Event capture started, writing to: {}", output_file);
    
    // Create or open the output file
    let mut file = match File::create(output_file) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to create output file: {}", e);
            std::process::exit(1);
        }
    };
    
    // Define the callback function for all events
    let callback = move |event: Event| {
        // Format the event as a string
        let event_str = format!("{:?}", event);
        
        // Write the event to the file
        if let Err(e) = writeln!(file, "{}", event_str) {
            eprintln!("Failed to write event to file: {}", e);
        }
        
        // Flush the file to ensure the event is written immediately
        if let Err(e) = file.flush() {
            eprintln!("Failed to flush file: {}", e);
        }
    };
    
    // Start listening for all events
    println!("Starting rdev::listen...");
    if let Err(error) = listen(callback) {
        eprintln!("Error: {:?}", error);
        std::process::exit(1);
    }
    
    // This will never be reached because listen blocks
    println!("Event capture stopped");
} 