// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::{File, create_dir_all};
use std::io::{Write, Read, Seek};
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use chrono::Local;
use tauri::State;
use scap::{
    capturer::{Point, Area, Size, Capturer, Options},
    frame::Frame,
};
use std::process::Command;
use rdev;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingOptions {
    pub fps: u32,
    pub show_cursor: bool,
    pub show_highlight: bool,
    pub save_frames: bool,
    pub capture_keystrokes: bool,
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
    keystrokes: Arc<Mutex<Vec<(String, Instant)>>>,
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

    // Create output directory for events and frames if needed
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let recordings_dir = PathBuf::from(home_dir).join("recordings");
    create_dir_all(&recordings_dir).expect("Failed to create recordings directory");

    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let session_dir = recordings_dir.join(timestamp);
    create_dir_all(&session_dir).expect("Failed to create session directory");
    println!("Created output directory: {:?}", session_dir);

    // Always set the output directory for events
    *state.output_dir.lock().unwrap() = Some(session_dir.clone());

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

    // Clear previous keystrokes if any
    if options.capture_keystrokes {
        println!("Clearing previous keystrokes...");
        *state.keystrokes.lock().unwrap() = Vec::new();
    }

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

    // Start keystroke capture thread if needed
    if options.capture_keystrokes {
        println!("Starting event capture thread...");
        let is_recording = state.is_recording.clone();
        let keystrokes = state.keystrokes.clone();
        let start_time = *state.start_time.lock().unwrap();
        let output_dir = state.output_dir.clone();

        thread::spawn(move || {
            println!("Event capture thread started");
            
            // Create a temporary file for events
            let events_file = if let Some(dir) = output_dir.lock().unwrap().as_ref() {
                dir.join("events_temp.txt")
            } else {
                println!("No output directory found for events");
                return;
            };
            
            // Start a separate process for event capture
            let mut child = match std::process::Command::new("cargo")
                .args([
                    "run",
                    "--example",
                    "event_capture",
                    "--",
                    events_file.to_str().unwrap(),
                ])
                .spawn() {
                    Ok(child) => child,
                    Err(e) => {
                        eprintln!("Failed to start event capture process: {}", e);
                        return;
                    }
                };
            
            println!("Event capture process started with PID: {}", child.id());
            
            // Monitor the events file and add events to our keystrokes list
            let mut last_read_position = 0;
            
            while is_recording.load(Ordering::SeqCst) {
                // Check if the events file exists
                if !events_file.exists() {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                
                // Read new events from the file
                match std::fs::File::open(&events_file) {
                    Ok(mut file) => {
                        let metadata = match file.metadata() {
                            Ok(metadata) => metadata,
                            Err(e) => {
                                eprintln!("Failed to get file metadata: {}", e);
                                thread::sleep(Duration::from_millis(100));
                                continue;
                            }
                        };
                        
                        let file_size = metadata.len() as usize;
                        
                        // If the file has grown, read the new content
                        if file_size > last_read_position {
                            // Seek to the last read position
                            if let Err(e) = file.seek(std::io::SeekFrom::Start(last_read_position as u64)) {
                                eprintln!("Failed to seek in file: {}", e);
                                thread::sleep(Duration::from_millis(100));
                                continue;
                            }
                            
                            // Read the new content
                            let mut buffer = vec![0; file_size - last_read_position];
                            match file.read(&mut buffer) {
                                Ok(bytes_read) => {
                                    if bytes_read > 0 {
                                        // Parse the events and add them to our keystrokes list
                                        let content = String::from_utf8_lossy(&buffer[..bytes_read]);
                                        for line in content.lines() {
                                            let timestamp = Instant::now();
                                            if let Ok(mut keystrokes) = keystrokes.lock() {
                                                keystrokes.push((line.to_string(), timestamp));
                                            }
                                        }
                                        
                                        // Update the last read position
                                        last_read_position += bytes_read;
                                    }
                                },
                                Err(e) => {
                                    eprintln!("Failed to read from file: {}", e);
                                }
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to open events file: {}", e);
                    }
                }
                
                thread::sleep(Duration::from_millis(50));
            }
            
            // Kill the event capture process
            if let Err(e) = child.kill() {
                eprintln!("Failed to kill event capture process: {}", e);
            }
            
            println!("Event capture thread stopped");
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
            
            // Save events to a file if any were captured
            let keystrokes = state.keystrokes.lock().unwrap();
            if !keystrokes.is_empty() {
                println!("Saving {} events to file...", keystrokes.len());
                let events_file = dir.join("events.txt");
                
                match File::create(&events_file) {
                    Ok(mut file) => {
                        for (event, timestamp) in keystrokes.iter() {
                            if let Err(e) = writeln!(file, "{}: {:?}", event, timestamp) {
                                eprintln!("Failed to write event to file: {}", e);
                            }
                        }
                        println!("Events saved to: {:?}", events_file);
                    },
                    Err(e) => {
                        eprintln!("Failed to create events file: {}", e);
                    }
                }
            } else {
                println!("No events to save.");
            }
            
            // Try to compile frames to video if frames were saved
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
    
    // Try a different approach with ffmpeg
    // First, let's try to convert the raw frames to PNG files
    println!("Converting raw frames to PNG format...");
    
    // Create a temporary directory for PNG files
    let temp_dir = frames_dir.join("temp_png");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp directory: {}", e))?;
    
    // Convert each raw frame to PNG
    for i in 0..frame_count {
        let raw_path = frames_dir.join(format!("frame_{:06}.raw", i));
        let png_path = temp_dir.join(format!("frame_{:06}.png", i));
        
        // Use ffmpeg to convert raw to PNG
        let convert_status = Command::new("ffmpeg")
            .args([
                "-f", "rawvideo",
                "-pix_fmt", "bgra",
                "-s", "1280x720",
                "-i", raw_path.to_str().ok_or("Invalid raw path")?,
                "-frames:v", "1",
                png_path.to_str().ok_or("Invalid PNG path")?,
            ])
            .output()
            .map_err(|e| e.to_string())?;
        
        if !convert_status.status.success() {
            let error_output = String::from_utf8_lossy(&convert_status.stderr);
            println!("Warning: Failed to convert frame {} to PNG: {}", i, error_output);
        }
    }
    
    // Now compile the PNG files to video
    println!("Compiling PNG frames to video...");
    let png_pattern = temp_dir.join("frame_%06d.png");
    
    let status = Command::new("ffmpeg")
        .args([
            "-framerate", &frame_rate.to_string(),
            "-i", png_pattern.to_str().ok_or("Invalid PNG pattern path")?,
            "-c:v", "libx264",
            "-preset", "medium",
            "-crf", "23",
            "-pix_fmt", "yuv420p",
            output_video.to_str().ok_or("Invalid output path")?,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    
    // Clean up temporary directory
    if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
        println!("Warning: Failed to clean up temporary directory: {}", e);
    }
    
    if !status.status.success() {
        let error_output = String::from_utf8_lossy(&status.stderr);
        println!("ffmpeg error output: {}", error_output);
        return Err(format!("Failed to compile video with ffmpeg: {}", error_output));
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
            keystrokes: Arc::new(Mutex::new(Vec::new())),
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
