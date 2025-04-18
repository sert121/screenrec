// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use chrono::Local;
use tauri::State;
use scap::{
    capturer::{Point, Area, Size, Capturer, Options},
    frame::Frame,
};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingOptions {
    pub fps: u32,
    pub show_cursor: bool,
    pub show_highlight: bool,
    pub save_frames: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingState {
    pub is_recording: bool,
    pub duration: u64,
    pub error: Option<String>,
}

struct RecordingStateWrapper {
    capturer: Arc<Mutex<Option<Capturer>>>,
    start_time: Mutex<Option<Instant>>,
    is_recording: Arc<AtomicBool>,
    output_dir: Arc<Mutex<Option<PathBuf>>>,
}

#[tauri::command]
fn start_recording(
    state: State<RecordingStateWrapper>,
    options: RecordingOptions,
) -> Result<(), String> {
    println!("Starting recording with options: {:?}", options);
    let mut recording_state = state.capturer.lock().unwrap();
    
    // Check if already recording
    if recording_state.is_some() {
        return Err("Recording is already in progress".to_string());
    }

    // Check if platform is supported
    if !scap::is_supported() {
        return Err("Platform not supported".to_string());
    }

    // Check and request permissions
    if !scap::has_permission() {
        println!("Permission not granted. Requesting permission...");
        if !scap::request_permission() {
            return Err("Permission denied".to_string());
        }
        println!("Permission granted.");
    } else {
        println!("Permission already granted.");
    }

    // Create output directory if saving frames
    if options.save_frames {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let recordings_dir = PathBuf::from(home_dir).join("recordings");
        create_dir_all(&recordings_dir).expect("Failed to create recordings directory");

        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let session_dir = recordings_dir.join(timestamp);
        create_dir_all(&session_dir).expect("Failed to create session directory");
        println!("Created output directory: {:?}", session_dir);

        *state.output_dir.lock().unwrap() = Some(session_dir);
    }

    // Create scap options
    let scap_options = Options {
        fps: options.fps,
        target: None, // None captures the primary display
        show_cursor: options.show_cursor,
        show_highlight: options.show_highlight,
        excluded_targets: None,
        output_type: scap::frame::FrameType::BGRAFrame,
        output_resolution: scap::capturer::Resolution::_720p,
        crop_area: None, // No cropping for now
        ..Default::default()
    };
    println!("Created scap options: {:?}", scap_options);

    // Create and store the capturer
    println!("Building capturer...");
    let mut capturer = match Capturer::build(scap_options) {
        Ok(capturer) => {
            println!("Capturer built successfully.");
            capturer
        },
        Err(e) => {
            println!("Failed to build capturer: {}", e);
            return Err(format!("Failed to build capturer: {}", e));
        }
    };
    
    println!("Starting capture...");
    capturer.start_capture();
    println!("Capture started.");
    *recording_state = Some(capturer);

    // Set recording state
    state.is_recording.store(true, Ordering::SeqCst);
    *state.start_time.lock().unwrap() = Some(Instant::now());
    println!("Recording state set to recording.");

    // Start frame saving thread if needed
    if options.save_frames {
        println!("Starting frame saving thread...");
        let is_recording = state.is_recording.clone();
        let capturer_clone = state.capturer.clone();
        let output_dir = state.output_dir.clone();
        let fps = options.fps;

        thread::spawn(move || {
            let mut frame_count = 0;
            let mut consecutive_errors = 0;
            const MAX_CONSECUTIVE_ERRORS: u32 = 10;
            
            while is_recording.load(Ordering::SeqCst) {
                // Get a reference to the capturer
                let capturer_guard = match capturer_clone.lock() {
                    Ok(guard) => guard,
                    Err(e) => {
                        eprintln!("Failed to lock capturer: {}", e);
                        thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                };
                
                // Check if capturer exists
                if let Some(capturer) = capturer_guard.as_ref() {
                    // We need to clone the capturer to use it in this thread
                    // This is a workaround for the mutability issue
                    let capturer_clone = capturer.clone();
                    
                    match capturer_clone.get_next_frame() {
                        Ok(frame) => {
                            // Process the frame based on its type
                            let buffer = match frame {
                                Frame::BGRA(frame) => {
                                    // For BGRA frames, we can use the data directly
                                    frame.data
                                },
                                Frame::BGR0(frame) => {
                                    // For BGR0 frames, we need to convert to BGRA
                                    // This is a simplified example - you might need to adjust based on actual frame format
                                    frame.data
                                },
                                _ => {
                                    // For other frame types, we'll skip for now
                                    eprintln!("Unsupported frame type");
                                    consecutive_errors += 1;
                                    continue;
                                }
                            };
                            
                            // Get a reference to the output directory
                            let output_dir_guard = match output_dir.lock() {
                                Ok(guard) => guard,
                                Err(e) => {
                                    eprintln!("Failed to lock output directory: {}", e);
                                    consecutive_errors += 1;
                                    continue;
                                }
                            };
                            
                            if let Some(dir) = output_dir_guard.as_ref() {
                                let frame_path = dir.join(format!("frame_{:06}.raw", frame_count));
                                match File::create(frame_path) {
                                    Ok(mut file) => {
                                        if let Err(e) = file.write_all(&buffer) {
                                            eprintln!("Failed to write frame {}: {}", frame_count, e);
                                            consecutive_errors += 1;
                                        } else {
                                            consecutive_errors = 0;
                                            frame_count += 1;
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!("Failed to create frame file {}: {}", frame_count, e);
                                        consecutive_errors += 1;
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            eprintln!("Failed to capture frame: {}", e);
                            consecutive_errors += 1;
                        }
                    }
                    
                    // If we have too many consecutive errors, wait a bit longer before trying again
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        eprintln!("Too many consecutive errors, waiting before retrying...");
                        thread::sleep(Duration::from_secs(1));
                        consecutive_errors = 0;
                    }
                }
                
                thread::sleep(Duration::from_millis(1000 / fps as u64));
            }
            
            println!("Frame saving thread stopped. Saved {} frames.", frame_count);
        });
    }

    println!("Recording started successfully.");
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<RecordingStateWrapper>) -> Result<String, String> {
    println!("Stopping recording...");
    let mut recording_state = state.capturer.lock().unwrap();
    
    if let Some(capturer) = recording_state.as_mut() {
        println!("Stopping capturer...");
        capturer.stop_capture();
        *recording_state = None;
        
        // Reset recording state
        state.is_recording.store(false, Ordering::SeqCst);
        *state.start_time.lock().unwrap() = None;
        
        // Return output directory path if frames were saved
        if let Some(dir) = state.output_dir.lock().unwrap().as_ref() {
            println!("Output directory found: {:?}", dir);
            
            // Try to compile frames to video
            let output_video = dir.join("output.mp4");
            println!("Attempting to compile video to: {:?}", output_video);
            
            match compile_frames_to_video(dir, &output_video, 30) {
                Ok(_) => {
                    println!("Video compilation successful");
                    Ok(output_video.to_string_lossy().to_string())
                },
                Err(e) => {
                    eprintln!("Failed to compile video: {}", e);
                    // Continue anyway, return the directory path
                    println!("Returning frames directory instead: {:?}", dir);
                    Ok(dir.to_string_lossy().to_string())
                }
            }
        } else {
            println!("No output directory found");
            Ok("".to_string())
        }
    } else {
        println!("No recording in progress");
        Err("No recording in progress".to_string())
    }
}

// Helper function to compile frames to video
fn compile_frames_to_video(frames_dir: &PathBuf, output_video: &PathBuf, frame_rate: i32) -> Result<(), String> {
    println!("Compiling frames to video...");
    println!("Frames directory: {:?}", frames_dir);
    println!("Output video: {:?}", output_video);
    
    // Check if frames directory exists
    if !frames_dir.exists() {
        return Err(format!("Frames directory not found: {:?}", frames_dir));
    }
    
    // Check if there are any frames
    let frame_count = std::fs::read_dir(frames_dir)
        .map_err(|e| format!("Failed to read frames directory: {}", e))?
        .filter(|entry| {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    return file_type.is_file() && entry.file_name().to_string_lossy().starts_with("frame_");
                }
            }
            false
        })
        .count();
    
    if frame_count == 0 {
        return Err("No frames found to compile".to_string());
    }
    
    println!("Found {} frames to compile", frame_count);
    
    // Create input pattern for ffmpeg
    let input_pattern = frames_dir.join("frame_%06d.raw");
    println!("Input pattern: {:?}", input_pattern);
    
    // Check if ffmpeg is available
    let ffmpeg_check = Command::new("ffmpeg")
        .arg("-version")
        .output();
    
    if ffmpeg_check.is_err() {
        return Err("ffmpeg not found. Please install ffmpeg to compile frames to video.".to_string());
    }
    
    println!("ffmpeg is available, starting compilation...");
    
    // Run ffmpeg to compile frames to video
    // Note: We're assuming BGRA format (32-bit per pixel)
    let status = Command::new("ffmpeg")
        .args([
            "-framerate", &frame_rate.to_string(),
            "-i", input_pattern.to_str().ok_or("Invalid input pattern path")?,
            "-f", "rawvideo",
            "-pix_fmt", "bgra",
            "-s", "1280x720", // Assuming 720p resolution
            "-c:v", "libx264",
            "-preset", "medium", // Add a preset for better encoding
            "-crf", "23", // Add a quality setting
            "-pix_fmt", "yuv420p",
            output_video.to_str().ok_or("Invalid output path")?,
        ])
        .status()
        .map_err(|e| e.to_string())?;
    
    if !status.success() {
        return Err("Failed to compile video with ffmpeg.".to_string());
    }
    
    println!("Video compilation completed successfully");
    Ok(())
}

#[tauri::command]
fn get_recording_state(state: State<RecordingStateWrapper>) -> RecordingState {
    let is_recording = state.is_recording.load(Ordering::SeqCst);
    let duration = if let Some(start_time) = *state.start_time.lock().unwrap() {
        start_time.elapsed().as_secs()
    } else {
        0
    };
    
    RecordingState {
        is_recording,
        duration,
        error: None,
    }
}

#[tauri::command]
fn get_platform() -> String {
    let platform = std::env::consts::OS;
    println!("Detected platform: {}", platform);
    platform.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RecordingStateWrapper {
            capturer: Arc::new(Mutex::new(None)),
            start_time: Mutex::new(None),
            is_recording: Arc::new(AtomicBool::new(false)),
            output_dir: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_recording_state,
            get_platform
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
