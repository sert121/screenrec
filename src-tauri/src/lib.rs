// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Screen capture ➕ FFmpeg piping with isolated event helper
//! ---------------------------------------------------------
//! • Video capture runs in threads with a bounded channel (max 4 frames).
//! • Events captured by a separate helper process (`event_capture` example) to avoid macOS CGEventTap aborts.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use crossbeam_channel::bounded;
use serde::{Deserialize, Serialize};
use tauri::State;

use scap::{capturer::Capturer, frame::Frame, is_supported, request_permission};

// -----------------------------------------------------------------------------
// Configuration structs
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingOptions {
    pub fps: u32,
    pub show_cursor: bool,
    pub show_highlight: bool,
    pub capture_keystrokes: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecordingState {
    pub is_recording: bool,
    pub duration: u64,
    pub error: Option<String>,
}

// -----------------------------------------------------------------------------
// Shared application state
// -----------------------------------------------------------------------------

struct AppState {
    is_recording: Arc<AtomicBool>,
    started_at:   Arc<Mutex<Option<Instant>>>,
    output_dir:   Arc<Mutex<Option<PathBuf>>>,
    ffmpeg:       Arc<Mutex<Option<Child>>>,
    helper:       Arc<Mutex<Option<Child>>>, // helper process for event capture
}

// -----------------------------------------------------------------------------
// Tauri commands
// -----------------------------------------------------------------------------

#[tauri::command]
fn start_recording(state: State<AppState>, mut opts: RecordingOptions) -> Result<(), String> {
    if state.is_recording.load(Ordering::Relaxed) {
        return Err("Recording already running".into());
    }
    if !is_supported() {
        return Err("Screen capture unsupported on this platform".into());
    }
    if !request_permission() {
        return Err("Screen-record permission denied".into());
    }

    // create session directory
    // let session = PathBuf::from(std::env::var("HOME").unwrap_or(".".into()))
    //     .join("recordings")
    //     .join(Local::now().format("%Y%m%d_%H%M%S").to_string());
    let home_dir = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let session = PathBuf::from(home_dir)
        .join("recordings")
        .join(Local::now().format("%Y%m%d_%H%M%S").to_string());
    std::fs::create_dir_all(&session).map_err(|e| e.to_string())?;
    *state.output_dir.lock().unwrap() = Some(session.clone());

    // spawn helper process for keystrokes/mouse events
    if opts.capture_keystrokes {
        let events_file = session.join("events.log");
        let helper = Command::new("cargo")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["run", "--example", "event_capture", "--", events_file.to_str().unwrap()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn event helper: {}", e))?;
        *state.helper.lock().unwrap() = Some(helper);
    }

    fn measure_max_fps(capturer: &mut Capturer) -> Result<f64, String> {
        const SAMPLE_FRAMES: usize = 30;
        let mut times = Vec::new();
    
        // warm up
        capturer.get_next_frame().map_err(|e| e.to_string())?;
    
        for _ in 0..SAMPLE_FRAMES {
            let start = Instant::now();
            capturer.get_next_frame().map_err(|e| e.to_string())?;
            let elapsed = start.elapsed().as_secs_f64();
            times.push(elapsed);
        }
    
        let avg_frame_time = times.iter().sum::<f64>() / times.len() as f64;
        let fps = 1.0 / avg_frame_time;
        Ok(fps)
    }
    

    // initialize capturer
    let mut capturer = Capturer::build(scap::capturer::Options {
        fps: opts.fps,
        target: None,
        show_cursor: opts.show_cursor,
        show_highlight: opts.show_highlight,
        output_type: scap::frame::FrameType::BGRAFrame,
        ..Default::default()
    }).map_err(|e| e.to_string())?;
    capturer.start_capture();
    let measured_fps = measure_max_fps(&mut capturer)?;
    println!("Measured max FPS: {:.2}", measured_fps);


        // Force final fps to minimum of measured_fps and 20
    opts.fps = opts.fps.min(measured_fps.floor() as u32);
    println!("Final recording FPS set to {}", opts.fps);

    // grab first frame for geometry
    let first = capturer.get_next_frame().map_err(|e| e.to_string())?;
    let (w, h) = match &first {
        Frame::BGRA(f) => (f.width, f.height),
        _ => return Err("Unexpected frame type".into()),
    };

    // launch ffmpeg
    let out_file = session.join("output.mp4");
    let mut ffmpeg = Command::new("ffmpeg")
        .args(["-y","-f","rawvideo","-pix_fmt","bgra",
               "-s", &format!("{w}x{h}"),
               "-r", &opts.fps.to_string(),
               "-i","-","-c:v","libx264","-preset","ultrafast",
               "-pix_fmt","yuv420p", out_file.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut ff_stdin = ffmpeg.stdin.take().ok_or("ffmpeg stdin unavailable")?;
    *state.ffmpeg.lock().unwrap() = Some(ffmpeg);

    // set up pipeline
    let (tx, rx) = bounded::<Vec<u8>>(4);
    let alive = state.is_recording.clone();
    alive.store(true, Ordering::Relaxed);

    // FFmpeg input thread
    let ffmpeg_alive = alive.clone();
    thread::spawn(move || {
        // Process all frames in the channel, even after stop signal
        while let Ok(buf) = rx.recv() {
            if ff_stdin.write_all(&buf).is_err() {
                break;
            }
        }
        // Ensure stdin is properly closed when we're done
        drop(ff_stdin);
    });

    // Frame capture thread
    let capture_alive = alive.clone();
    thread::spawn(move || {
        let dt = Duration::from_secs_f64(1.0 / opts.fps as f64);
        let recording_start = Instant::now();
        let mut frame_idx = 0u32;
        
        while capture_alive.load(Ordering::Relaxed) {
            let expected_time = recording_start + dt * frame_idx;
        
            let now = Instant::now();
            if now >= expected_time {
                if let Ok(Frame::BGRA(f)) = capturer.get_next_frame() {
                    if tx.send(f.data).is_err() {
                        break;
                    }
                }
                frame_idx += 1;
            } else {
                let sleep_time = expected_time - now;
                thread::sleep(sleep_time);
            }
        }
        // Channel will be closed when tx is dropped
    });

    *state.started_at.lock().unwrap() = Some(Instant::now());
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<AppState>) -> Result<String, String> {
    // First, signal threads to stop
    state.is_recording.store(false, Ordering::Relaxed);
    
    // Give time for the pipeline to finish (3 seconds should be enough)
    std::thread::sleep(std::time::Duration::from_secs(3));

    // kill helper and wait for it to exit
    if let Some(mut h) = state.helper.lock().unwrap().take() {
        h.kill().map_err(|e| format!("Failed to kill event capture: {}", e))?;
        match h.wait() {
            Ok(status) => {
                if !status.success() {
                    eprintln!("Event capture exited with status: {}", status);
                }
            }
            Err(e) => eprintln!("Failed to wait for event capture: {}", e),
        }
    }

    // Wait for ffmpeg to finish processing
    if let Some(mut c) = state.ffmpeg.lock().unwrap().take() {
        match c.wait() {
            Ok(status) => {
                if !status.success() {
                    eprintln!("FFmpeg exited with status: {}", status);
                }
            }
            Err(e) => {
                eprintln!("Failed to wait for ffmpeg: {}", e);
                // If waiting fails, then kill it
                let _ = c.kill();
            }
        }
    }

    // return path
    let out = state.output_dir.lock().unwrap().clone().unwrap().join("output.mp4");
    
    // Verify the file exists and has size > 0
    match std::fs::metadata(&out) {
        Ok(metadata) => {
            if metadata.len() == 0 {
                return Err("Recording failed: output file is empty".into());
            }
        }
        Err(e) => {
            return Err(format!("Recording failed: {}", e));
        }
    }

    Ok(out.to_string_lossy().into())
}

#[tauri::command]
fn get_recording_state(state: State<AppState>) -> RecordingState {
    RecordingState {
        is_recording: state.is_recording.load(Ordering::Relaxed),
        duration: state.started_at.lock().unwrap().map(|t| t.elapsed().as_secs()).unwrap_or(0),
        error: None,
    }
}

#[tauri::command]
fn get_platform() -> String { std::env::consts::OS.into() }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            is_recording: Arc::new(AtomicBool::new(false)),
            started_at:   Arc::new(Mutex::new(None)),
            output_dir:   Arc::new(Mutex::new(None)),
            ffmpeg:       Arc::new(Mutex::new(None)),
            helper:       Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_recording_state,
            get_platform,
        ])
        .run(tauri::generate_context!())
        .expect("tauri run failed");
}
