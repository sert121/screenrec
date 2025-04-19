use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use rdev::{listen, Event, EventType};

fn main() {
    // first arg is the output path
    let path = env::args().nth(1)
        .expect("Usage: event_capture <events.log path>");
    let out_path = PathBuf::from(path);

    // create parent dir
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .write(true)
        .open(&out_path)
        .unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let alive = running.clone();

    ctrlc::set_handler(move || {
        alive.store(false, Ordering::Relaxed);
    }).unwrap();

    // run on main thread with CFRunLoop properly set up
    let _ = listen(move |ev: Event| {
        if !running.load(Ordering::Relaxed) { return; }
        
        let desc = match ev.event_type {
            EventType::KeyPress(k) => format!("KeyPress {k:?}"),
            EventType::KeyRelease(k) => format!("KeyRelease {k:?}"),
            EventType::ButtonPress(b) => format!("MouseDown {b:?}"),
            EventType::ButtonRelease(b) => format!("MouseUp {b:?}"),
            EventType::MouseMove { x, y } => format!("MouseMove {x:.0},{y:.0}"),
            EventType::Wheel { delta_x, delta_y } => format!("Wheel {delta_x},{delta_y}"),
        };

        let _ = writeln!(file, "{:?}: {}", ev.time, desc);
    });
}
