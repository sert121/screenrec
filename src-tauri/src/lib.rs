// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_platform() -> String {
    let platform = std::env::consts::OS;
    println!("Platform detected: {}", platform);
    platform.to_string()
}

#[tauri::command]
fn start_recording(options: serde_json::Value) -> Result<(), String> {
    println!("Starting recording with options: {:?}", options);
    // TODO: Implement actual recording logic
    Ok(())
}

#[tauri::command]
fn stop_recording() -> Result<String, String> {
    println!("Stopping recording");
    // TODO: Implement actual recording logic
    Ok("recording.mp4".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, get_platform, start_recording, stop_recording])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
