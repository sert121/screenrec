// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod recording;
use std::sync::Mutex;

use serde::{Serialize, Deserialize};
use recording::{Recording, RecordingOptions, RecordingState};
use std::sync::Arc;
use tauri::State;

// State to hold the recording instance
struct RecordingStateWrapper(Mutex<Option<Recording>>);

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[tauri::command]
fn get_platform() -> String {
    let platform = std::env::consts::OS;
    platform.to_string()
}

#[tauri::command]
fn start_recording(
    app_handle: tauri::AppHandle,
    state: State<RecordingStateWrapper>,
    options: RecordingOptions,
) -> Result<(), String> {
    let mut recording_state = state.0.lock().unwrap();
    
    // Create a new recording if one doesn't exist
    if recording_state.is_none() {
        *recording_state = Some(Recording::new(&app_handle));
    }
    
    // Start recording
    if let Some(recording) = recording_state.as_ref() {
        recording.start_recording(options)
    } else {
        Err("Failed to initialize recording".to_string())
    }
}

#[tauri::command]
fn stop_recording(state: State<RecordingStateWrapper>) -> Result<String, String> {
    let recording_state = state.0.lock().unwrap();
    
    if let Some(recording) = recording_state.as_ref() {
        recording.stop_recording()
    } else {
        Err("No recording in progress".to_string())
    }
}

#[tauri::command]
fn get_recording_state(state: State<RecordingStateWrapper>) -> Result<RecordingState, String> {
    let recording_state = state.0.lock().unwrap();
    
    if let Some(recording) = recording_state.as_ref() {
        Ok(recording.get_state())
    } else {
        Ok(RecordingState {
            is_recording: false,
            duration: 0,
            output_path: None,
            error: None,
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RecordingStateWrapper(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            greet,
            get_platform,
            start_recording,
            stop_recording,
            get_recording_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
