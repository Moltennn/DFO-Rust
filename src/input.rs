//! Raw mouse input from evdev.
//!
//! Wayland's relative-pointer stream is quantised to surface pixels: at slow
//! speed the game receives 0/1-pixel steps, and the same hand motion yields
//! about 2.5x fewer counts than the desktop game gets from the raw device.
//! Reading the mouse's evdev node gives the device's own counts, unaccelerated.
//!
//! The page connects to `/__input` and replaces WebKit's pointer-lock deltas
//! with these while the pointer is locked. Without permission to read an input
//! device, nothing starts and the shell keeps the compositor stream.

use evdev::{Device, EventType, KeyCode, RelativeAxisCode, SynchronizationCode};
use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug)]
pub struct Delta {
    pub dx: f64,
    pub dy: f64,
    /// DOM `MouseEvent.buttons` mask: 1 left, 2 right, 4 middle, 8/16 side.
    pub buttons: u32,
}

pub struct RawInput {
    pub enabled: bool,
    pub available: bool,
    pub devices: Vec<String>,
    pub tx: broadcast::Sender<Delta>,
    /// Held so the readers stay alive for the process lifetime.
    _readers: Vec<std::thread::JoinHandle<()>>,
}

pub fn start(enabled: bool) -> RawInput {
    let (tx, _) = broadcast::channel(4096);
    let mut devices = Vec::new();
    let mut readers = Vec::new();
    if enabled {
        for (path, device) in evdev::enumerate() {
            let Some(axes) = device.supported_relative_axes() else {
                continue;
            };
            if !axes.contains(RelativeAxisCode::REL_X) || !axes.contains(RelativeAxisCode::REL_Y) {
                continue;
            }
            let Some(keys) = device.supported_keys() else {
                continue;
            };
            if !keys.contains(KeyCode::BTN_LEFT) {
                continue;
            }
            let name = device.name().unwrap_or("?").to_string();
            devices.push(format!("{} ({name})", path.display()));
            let tx = tx.clone();
            readers.push(std::thread::spawn(move || read_device(device, tx)));
        }
    }
    RawInput {
        enabled,
        available: !devices.is_empty(),
        devices,
        tx,
        _readers: readers,
    }
}

fn read_device(mut device: Device, tx: broadcast::Sender<Delta>) {
    let mut dx = 0i32;
    let mut dy = 0i32;
    let mut buttons = 0u32;
    let mut changed = false;
    loop {
        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            Err(_) => break,
        };
        for event in events {
            match event.event_type() {
                EventType::RELATIVE if event.code() == RelativeAxisCode::REL_X.0 => {
                    dx += event.value();
                    changed = true;
                }
                EventType::RELATIVE if event.code() == RelativeAxisCode::REL_Y.0 => {
                    dy += event.value();
                    changed = true;
                }
                EventType::KEY => {
                    let bit = match event.code() {
                        code if code == KeyCode::BTN_LEFT.0 => 1,
                        code if code == KeyCode::BTN_RIGHT.0 => 2,
                        code if code == KeyCode::BTN_MIDDLE.0 => 4,
                        code if code == KeyCode::BTN_SIDE.0 => 8,
                        code if code == KeyCode::BTN_EXTRA.0 => 16,
                        _ => 0,
                    };
                    if bit != 0 {
                        if event.value() == 1 {
                            buttons |= bit;
                        } else {
                            buttons &= !bit;
                        }
                        changed = true;
                    }
                }
                EventType::SYNCHRONIZATION
                    if event.code() == SynchronizationCode::SYN_REPORT.0 && changed =>
                {
                    let _ = tx.send(Delta {
                        dx: dx as f64,
                        dy: dy as f64,
                        buttons,
                    });
                    dx = 0;
                    dy = 0;
                    changed = false;
                }
                _ => {}
            }
        }
    }
}
