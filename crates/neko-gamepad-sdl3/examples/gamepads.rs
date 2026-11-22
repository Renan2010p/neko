//! List gamepads and print their input for a few seconds.
//!
//! `cargo run -p neko-gamepad-sdl3 --example gamepads [seconds] [--rumble]`

use std::time::{Duration, Instant};

use neko_gamepad_sdl3::Gamepads;
use neko_platform::{Event, GamepadEvent};

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let rumble = std::env::args().any(|a| a == "--rumble");
    let mut pads = match Gamepads::standalone() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SDL3 gamepads unavailable: {e}");
            std::process::exit(1);
        }
    };
    let end = Instant::now() + Duration::from_secs(secs);
    let mut events = Vec::new();
    let mut rumbled = false;
    while Instant::now() < end {
        pads.poll(&mut events);
        for e in events.drain(..) {
            if let Event::Gamepad { id, event } = e {
                match event {
                    GamepadEvent::Axis { axis, value } if value.abs() > 0.5 => println!("pad {id}: {axis:?} = {value:.2}"),
                    GamepadEvent::Axis { .. } => {}
                    other => println!("pad {id}: {other:?}"),
                }
            }
        }
        if rumble && !rumbled && !pads.is_empty() {
            rumbled = true;
            println!("rumble on pad 0: {}", pads.rumble(0, 0.3, 0.5, 200));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    println!("connected at exit: {:?}", pads.connected().iter().map(|(id, i)| (id, &i.name)).collect::<Vec<_>>());
}
