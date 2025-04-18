use std::sync::Arc;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::{File, create_dir_all};
use std::io::Write;
use serde::{Serialize, Deserialize};
use screenshots::Screen;
use std::sync::Mutex;
use std::thread;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use rdev::{listen, Event, EventType};
use chrono::Local;
use serde_json;
use tauri::AppHandle;
use std::env;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingOptions {
    pub audio: bool,
    pub video: bool,
    pub frame_rate: i32,
    pub quality: String,
    pub fps: u32,
    pub output_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingState {
    pub is_recording: bool,
    pub duration: u64,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

pub struct Recording {
    is_recording: Arc<AtomicBool>,
    start_time: Arc<Mutex<Option<Instant>>>,
    options: Arc<Mutex<RecordingOptions>>,
    screenshots_dir: PathBuf,
    mouse_events_file: PathBuf,
}

impl Recording {
    pub fn new(_app_handle: &AppHandle) -> Self {
        // Create a recordings directory in the user's home directory
        let home_dir = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let recordings_dir = PathBuf::from(home_dir).join("recordings");
        create_dir_all(&recordings_dir).expect("Failed to create recordings directory");

        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let session_dir = recordings_dir.join(timestamp);
        create_dir_all(&session_dir).expect("Failed to create session directory");

        let screenshots_dir = session_dir.join("screenshots");
        create_dir_all(&screenshots_dir).expect("Failed to create screenshots directory");

        let mouse_events_file = session_dir.join("mouse_events.json");
        let mut file = File::create(&mouse_events_file).expect("Failed to create mouse events file");
        writeln!(file, "[]").expect("Failed to write to mouse events file");

        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            start_time: Arc::new(Mutex::new(None)),
            options: Arc::new(Mutex::new(RecordingOptions {
                audio: true,
                video: true,
                frame_rate: 30,
                quality: "high".to_string(),
                fps: 30,
                output_path: session_dir.to_string_lossy().to_string(),
            })),
            screenshots_dir,
            mouse_events_file,
        }
    }

    pub fn start_recording(&self, options: RecordingOptions) -> Result<(), String> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err("Recording is already in progress".to_string());
        }

        *self.options.lock().unwrap() = options;
        self.is_recording.store(true, Ordering::SeqCst);
        *self.start_time.lock().unwrap() = Some(Instant::now());

        let is_recording = self.is_recording.clone();
        let mouse_events_file = self.mouse_events_file.clone();

        std::thread::spawn(move || {
            if let Err(e) = listen(move |event: Event| {
                if !is_recording.load(Ordering::SeqCst) {
                    return;
                }
                if let EventType::MouseMove { x, y } = event.event_type {
                    if let Ok(mut file) = File::options().append(true).open(&mouse_events_file) {
                        let event_json = serde_json::json!({
                            "type": "mouse_move",
                            "x": x,
                            "y": y,
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or(Duration::from_secs(0))
                                .as_secs_f64()
                        });
                        if let Ok(json_str) = serde_json::to_string(&event_json) {
                            let _ = writeln!(file, "{}", json_str);
                        }
                    }
                }
            }) {
                eprintln!("Error listening to mouse events: {:?}", e);
            }
        });

        let is_recording = self.is_recording.clone();
        let start_time = self.start_time.clone();
        let options = self.options.clone();
        let screenshots_dir = self.screenshots_dir.clone();

        std::thread::spawn(move || {
            if let Err(e) = Self::record_screen(
                is_recording,
                start_time,
                options,
                screenshots_dir,
            ) {
                eprintln!("Error recording screen: {:?}", e);
            }
        });

        Ok(())
    }

    pub fn stop_recording(&self) -> Result<String, String> {
        if !self.is_recording.load(Ordering::SeqCst) {
            return Err("No recording in progress".to_string());
        }

        self.is_recording.store(false, Ordering::SeqCst);

        let output_path = self.options.lock().unwrap().output_path.clone();
        let screenshots_dir = PathBuf::from(format!("{}/screenshots", output_path));
        let output_video = PathBuf::from(format!("{}/output.mp4", output_path));

        Self::compile_frames_to_video(&screenshots_dir, &output_video, 30)?;

        Ok(output_video.to_string_lossy().to_string())
    }

    pub fn get_state(&self) -> RecordingState {
        let is_recording = self.is_recording.load(Ordering::SeqCst);
        let duration = if let Some(start_time) = *self.start_time.lock().unwrap() {
            start_time.elapsed().as_secs()
        } else {
            0
        };

        let output_path = if is_recording {
            Some(self.options.lock().unwrap().output_path.clone())
        } else {
            None
        };

        RecordingState {
            is_recording,
            duration,
            output_path,
            error: None,
        }
    }

    fn record_screen(
        is_recording: Arc<AtomicBool>,
        start_time: Arc<Mutex<Option<Instant>>>,
        options: Arc<Mutex<RecordingOptions>>,
        screenshots_dir: PathBuf,
    ) -> Result<(), String> {
        let screens = Screen::all().map_err(|e| format!("Failed to get screen: {}", e))?;
        if screens.is_empty() {
            return Err("No screens found".to_string());
        }

        let screen = &screens[0];
        let fps = options.lock().unwrap().fps;
        let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);

        let mut frame_count = 0;

        while is_recording.load(Ordering::SeqCst) {
            let frame_start = Instant::now();

            if let Ok(image) = screen.capture() {
                let frame_path = screenshots_dir.join(format!("frame_{:06}.png", frame_count));
                if let Err(e) = image.save(&frame_path) {
                    eprintln!("Failed to save frame {}: {}", frame_count, e);
                }
                frame_count += 1;
            } else {
                eprintln!("Failed to capture frame {}", frame_count);
            }

            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                thread::sleep(frame_duration - elapsed);
            }
        }

        Ok(())
    }

    fn compile_frames_to_video(screenshots_dir: &PathBuf, output_video: &PathBuf, frame_rate: i32) -> Result<(), String> {
        let input_pattern = screenshots_dir.join("frame_%06d.png");

        let status = Command::new("ffmpeg")
            .args([
                "-framerate", &frame_rate.to_string(),
                "-i", input_pattern.to_str().ok_or("Invalid input pattern path")?,
                "-c:v", "libx264",
                "-pix_fmt", "yuv420p",
                output_video.to_str().ok_or("Invalid output path")?,
            ])
            .status()
            .map_err(|e| e.to_string())?;

        if !status.success() {
            return Err("Failed to compile video with ffmpeg.".to_string());
        }

        Ok(())
    }
}
