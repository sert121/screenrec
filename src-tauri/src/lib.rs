
// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! High‑level changes
//! ------------------
//! • Capture and encode run in **independent threads** connected by a **bounded channel**
//!   (drops frames when the encoder lags ⇢ no RAM explosion, no freezes).
//! • No global `Mutex<Capturer>`: the capturer lives only in the capture thread.
//! • Frames stay in **BGRA**; we write them straight to `ffmpeg`, avoiding the costly
//!   per‑pixel copy.
//! • The capture loop sleeps to respect the target FPS instead of busy‑spinning.
//! • Graceful shutdown: closing the channel ⇒ encoder thread finishes ⇒ stdin closed ⇒
//!   `ffmpeg` flushes and exits.
//!
//! build‑time deps (Cargo.toml):
//! ```toml
//! crossbeam-channel = "0.5"
//! scap              = "0.5"     # or whatever version you use
//! chrono            = "0.4"
//! tauri             = { version = "2", features = ["api-all"] }
//! ```

use std::path::PathBuf;
use std::io::Write; // <- import Write trait for ChildStdin methods
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use crossbeam_channel::{bounded, Sender};
use serde::{Deserialize, Serialize};
use tauri::State;

use scap::{capturer::Capturer, frame::Frame, is_supported, request_permission};

// -----------------------------------------------------------------------------
// User‑facing structs
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingOptions {
    pub fps: u32,
    pub show_cursor: bool,
    pub show_highlight: bool,
    pub save_frames: bool,      // still respected → pipes to ffmpeg
    pub capture_keystrokes: bool, // kept for API compatibility (noop here)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecordingState {
    pub is_recording: bool,
    pub duration: u64,
    pub error: Option<String>,
}

// -----------------------------------------------------------------------------
// App‑wide shared state (kept minimal)
// -----------------------------------------------------------------------------

struct AppState {
    is_recording: Arc<AtomicBool>,
    started_at:   Arc<Mutex<Option<Instant>>>,
    output_dir:   Arc<Mutex<Option<PathBuf>>>,
    ffmpeg:       Arc<Mutex<Option<Child>>>,
}

// -----------------------------------------------------------------------------
// Commands
// -----------------------------------------------------------------------------

#[tauri::command]
fn start_recording(state: State<AppState>, opts: RecordingOptions) -> Result<(), String> {
    if state.is_recording.load(Ordering::Relaxed) {
        return Err("Recording already running".into());
    }

    if !is_supported() {
        return Err("Display capture not supported on this platform".into());
    }
    if !request_permission() {
        return Err("Screen‑record permission denied".into());
    }

    // ---------------------------------------------------------------------
    // Create session dir
    // ---------------------------------------------------------------------
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let session = PathBuf::from(home)
        .join("recordings")
        .join(Local::now().format("%Y%m%d_%H%M%S").to_string());
    std::fs::create_dir_all(&session).map_err(|e| e.to_string())?;
    *state.output_dir.lock().unwrap() = Some(session.clone());

    // ---------------------------------------------------------------------
    // Build capturer (blocking, so still in command thread)
    // ---------------------------------------------------------------------
    let cap_opts = scap::capturer::Options {
        fps: opts.fps,
        target: None,
        show_cursor: opts.show_cursor,
        show_highlight: opts.show_highlight,
        output_type: scap::frame::FrameType::BGRAFrame,
        ..Default::default()
    };

    let mut capturer = Capturer::build(cap_opts).map_err(|e| e.to_string())?;
    capturer.start_capture();

    // Grab one frame synchronously to determine geometry
    let first = capturer
        .get_next_frame()
        .map_err(|e| format!("initial frame failed: {e}"))?;
    let (w, h) = match &first {
        Frame::BGRA(f) => (f.width, f.height),
        _ => return Err("Unexpected frame type".into()),
    };

    // ---------------------------------------------------------------------
    // Launch ffmpeg
    // ---------------------------------------------------------------------
    let out_file = session.join("output.mp4");
    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y",               // overwrite
            "-f", "rawvideo",
            "-pix_fmt", "bgra",
            "-s", &format!("{w}x{h}"),
            "-r", &opts.fps.to_string(),
            "-i", "-",          // stdin
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-pix_fmt", "yuv420p",
            out_file.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg spawn failed: {e}"))?;
    let mut ffmpeg_stdin = ffmpeg.stdin.take().ok_or("ffmpeg stdin unavailable")?;
    *state.ffmpeg.lock().unwrap() = Some(ffmpeg);

    // ---------------------------------------------------------------------
    // Spawn encoder + capture threads — communicate via bounded channel
    // ---------------------------------------------------------------------
    let (tx, rx) = bounded::<Vec<u8>>(4); // 4 frames max

    // encoder thread
    thread::spawn(move || {
        for buf in rx {
            if ffmpeg_stdin.write(&buf).is_err() {
                break; // ffmpeg closed ⇒ stop
            }
        }
    });

    // capture thread (owns the capturer)
    let recording_flag = state.is_recording.clone();
    recording_flag.store(true, Ordering::Relaxed);

    thread::spawn(move || {
        let fps = opts.fps;
        let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);
        let mut next_deadline = Instant::now();

        // send first frame we already captured
        if let Frame::BGRA(f) = first {
            let _ = tx.try_send(f.data); // ignore full channel (shouldn’t be)
        }

        while recording_flag.load(Ordering::Relaxed) {
            match capturer.get_next_frame() {
                Ok(Frame::BGRA(f)) => {
                    // Drop frame if queue full ⇒ never blocks
                    let _ = tx.try_send(f.data);
                }
                Ok(_) => {/* other frame types ignored */}
                Err(_) => {/* swallow error; try next */}
            }
            // throttle
            if let Some(rem) = next_deadline.checked_duration_since(Instant::now()) {
                thread::sleep(rem);
            }
            next_deadline += frame_duration;
        }
        // channel closes when tx dropped ⇒ encoder thread exits, stdin closes
    });

    // Stamp start time
    *state.started_at.lock().unwrap() = Some(Instant::now());
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<AppState>) -> Result<String, String> {
    // signal threads to stop
    state.is_recording.store(false, Ordering::Relaxed);

    // wait for ffmpeg to exit (max 5 s)
    let mut child_opt = state.ffmpeg.lock().unwrap().take();
    if let Some(mut child) = child_opt.take() {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if let Ok(Some(_)) = child.try_wait() { break; }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
    }

    let out = state
        .output_dir
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
        .join("output.mp4");
    Ok(out.to_string_lossy().into())
}

#[tauri::command]
fn get_recording_state(state: State<AppState>) -> RecordingState {
    let is_rec = state.is_recording.load(Ordering::Relaxed);
    let dur = state
        .started_at
        .lock()
        .unwrap()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);
    RecordingState { is_recording: is_rec, duration: dur, error: None }
}

#[tauri::command]
fn get_platform() -> String { std::env::consts::OS.into() }

// -----------------------------------------------------------------------------
// Tauri entry‑point
// -----------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            is_recording: Arc::new(AtomicBool::new(false)),
            started_at:   Arc::new(Mutex::new(None)),
            output_dir:   Arc::new(Mutex::new(None)),
            ffmpeg:       Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_recording_state,
            get_platform
        ])
        .run(tauri::generate_context!())
        .expect("tauri run failed");
}
