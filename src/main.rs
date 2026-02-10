// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

mod cedar_client;
mod display;
mod prefs;
mod renderer;
mod ssd1306;
mod ssd1351;
mod web;

use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering},
    },
    time::Duration,
};

use cedar_client::{CedarClient, CedarResponse, ResponseStatus, ServerMode, ServerState};
use display::TargetDisplay;
use embedded_graphics::{
    draw_target::DrawTarget, geometry::OriginDimensions, pixelcolor::PixelColor,
};
use renderer::{DrawState, RotatedDisplay, Rotation, draw_ui};
use simple_signal::{self, Signal};
use tokio::time::sleep;
use web::{Framebuffer, ServerContext};

struct FakeStateProvider {
    tilt: f64,
    rot: f64,
    angle: f64,
    has_solution: bool,
    is_alt_az: bool,
}

impl FakeStateProvider {
    fn new() -> Self {
        FakeStateProvider {
            tilt: 123.8,
            rot: -45.3,
            angle: 0.0,
            has_solution: true,
            is_alt_az: true,
        }
    }

    fn get_next_response(&mut self) -> CedarResponse {
        self.angle = (self.angle + 9.0) % 360.0;
        let delta = if self.has_solution { 1.17 } else { 10.0 };
        self.tilt = self.tilt + delta;
        if self.tilt > 180.0 {
            self.tilt = -179.2;
        }
        self.rot = self.rot - delta * 2.0;
        if self.rot < -180.0 {
            self.rot = 179.7;
            if !self.has_solution {
                self.is_alt_az = !self.is_alt_az;
            }
            self.has_solution = !self.has_solution;
        }
        CedarResponse {
            status: ResponseStatus::Success,
            server_state: Some(ServerState {
                server_mode: ServerMode::Operating,
                is_alt_az: self.is_alt_az,
                has_slew_request: self.has_solution,
                rotation_target_distance: self.rot,
                tilt_target_distance: self.tilt,
                target_angle: self.angle,
                has_solution: self.has_solution,
            }),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    let mirror_enabled = args.contains("--mirror");
    let test_mode = args.contains("--test");

    let cli_brightness = match args.opt_value_from_str::<_, u32>("--brightness")? {
        Some(val) if (1..=255).contains(&val) => Some(val as u8),
        Some(_) => return Err("Brightness must be between 1 and 255".into()),
        None => None,
    };

    let cli_rotation = match args.opt_value_from_str::<_, u16>("--rotation")? {
        Some(val) if val == 0 || val == 90 || val == 180 || val == 270 => Some(val),
        Some(_) => return Err("Rotation must be one of 0, 90, 180, or 270".into()),
        None => None,
    };

    let display_type = match args.opt_value_from_str::<_, u32>("--type")? {
        Some(val) if (1..=3).contains(&val) => val,
        Some(_) => return Err("Type must be between 1 and 3".into()),
        None => 1,
    };

    let file_brightness = prefs::load_brightness();
    let initial_brightness = cli_brightness.unwrap_or(file_brightness);

    let file_rotation = prefs::load_rotation();
    let initial_rotation = cli_rotation.unwrap_or(file_rotation);
    let current_rotation = Rotation::from_degrees(initial_rotation);

    let shared_brightness = Arc::new(AtomicU8::new(initial_brightness));
    let shared_rotation = Arc::new(AtomicU16::new(initial_rotation));

    // Initialize shared frame with black pixels (128*128*2 bytes)
    let shared_frame = Arc::new(RwLock::new(vec![0u8; 128 * 128 * 2]));

    let server_ctx = ServerContext {
        brightness: shared_brightness.clone(),
        rotation: shared_rotation.clone(),
        frame: shared_frame.clone(),
    };

    web::start_server(server_ctx)?;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    simple_signal::set_handler(&[Signal::Int, Signal::Term], move |signal_rec| {
        println!("Signal received : '{:?}'", signal_rec);
        r.store(false, Ordering::SeqCst);
    });

    // Initialize and use specific display wrapper
    match display_type {
        1 => {
            let raw_disp = ssd1351::Ssd1351::new()?;
            let disp = RotatedDisplay::new_rgb_128_128(raw_disp, current_rotation);
            run_display(
                disp,
                running,
                shared_brightness,
                shared_rotation,
                test_mode,
                mirror_enabled,
                display_type,
                shared_frame,
            )
            .await?
        }
        2 => {
            let raw_disp = ssd1306::Ssd1306::new_128_64()?;
            let disp = RotatedDisplay::new_binary_128_64(raw_disp, current_rotation);
            run_display(
                disp,
                running,
                shared_brightness,
                shared_rotation,
                test_mode,
                mirror_enabled,
                display_type,
                shared_frame,
            )
            .await?
        }
        _ => {
            let raw_disp = ssd1306::Ssd1306::new_128_32()?;
            let disp = RotatedDisplay::new_binary_128_32(raw_disp, current_rotation);
            run_display(
                disp,
                running,
                shared_brightness,
                shared_rotation,
                test_mode,
                mirror_enabled,
                display_type,
                shared_frame,
            )
            .await?
        }
    };
    Ok(())
}

async fn run_display<D, C>(
    mut disp: RotatedDisplay<D, C>,
    running: Arc<AtomicBool>,
    shared_brightness: Arc<AtomicU8>,
    shared_rotation: Arc<AtomicU16>,
    test_mode: bool,
    mirror_enabled: bool,
    display_type: u32,
    shared_frame: Arc<RwLock<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    D: TargetDisplay + DrawTarget<Color = C> + OriginDimensions,
    C: PixelColor,
    D::Error: std::fmt::Debug,
{
    disp.parent.turn_on().await?;

    let initial_brightness = shared_brightness.load(Ordering::Relaxed);
    let mut current_brightness = initial_brightness;
    disp.parent.set_brightness(current_brightness).await?;

    let mut current_rotation = Rotation::from_degrees(shared_rotation.load(Ordering::Relaxed));

    // Virtual framebuffer for web rendering
    let mut web_fb = if mirror_enabled {
        Some(match display_type {
            1 => RotatedDisplay::new_rgb_128_128(Framebuffer::new(), current_rotation),
            2 => RotatedDisplay::new_rgb_128_64(Framebuffer::new(), current_rotation),
            _ => RotatedDisplay::new_rgb_128_32(Framebuffer::new(), current_rotation),
        })
    } else {
        None
    };

    let mut client = CedarClient::new();
    let mut last_slew: Option<ServerState> = None;
    let mut stale_angle = 0;

    let mut fake_provider = FakeStateProvider::new();

    while running.load(Ordering::SeqCst) {
        let target_brightness = shared_brightness.load(Ordering::Relaxed);
        if target_brightness != current_brightness {
            println!("Updating display brightness to {}", target_brightness);
            disp.parent.set_brightness(target_brightness).await?;
            current_brightness = target_brightness;
        }

        let target_rotation_deg = shared_rotation.load(Ordering::Relaxed);
        let target_rotation = Rotation::from_degrees(target_rotation_deg);
        if target_rotation != current_rotation {
            println!("Updating display rotation to {}", target_rotation_deg);
            disp.set_rotation(target_rotation);
            if let Some(fb) = &mut web_fb {
                fb.set_rotation(target_rotation);
            }
            current_rotation = target_rotation;
        }

        let resp = if test_mode {
            fake_provider.get_next_response()
        } else {
            client.get_state().await
        };
        let draw_state = if resp.status != ResponseStatus::Success {
            DrawState::Message(format!("{:?}", resp.status))
        } else if let Some(state) = &resp.server_state {
            match state.server_mode {
                ServerMode::Operating => {
                    if !state.has_slew_request {
                        if state.has_solution {
                            last_slew = None;
                        }
                        if let Some(slew) = &last_slew {
                            let state = DrawState::Operating(slew, Some(stale_angle));
                            stale_angle = (stale_angle + 9) % 360;
                            state
                        } else {
                            DrawState::Message("No Target".to_string())
                        }
                    } else {
                        last_slew = Some(state.clone());
                        DrawState::Operating(state, None)
                    }
                }
                ServerMode::Calibrating => DrawState::Message("Calibrating".to_string()),
                _ => DrawState::Message("Setup Mode".to_string()),
            }
        } else {
            DrawState::Message("...".to_string())
        };

        // Draw to physical display
        disp.clear();
        draw_ui(&mut disp, &draw_state);
        let _ = disp.parent.flush().await;

        // Draw to virtual framebuffer
        if mirror_enabled {
            if let Some(fb) = &mut web_fb {
                fb.clear();
                draw_ui(fb, &draw_state);

                if let Ok(mut lock) = shared_frame.write() {
                    lock.copy_from_slice(fb.parent.as_bytes());
                }
            }
        }

        sleep(Duration::from_millis(50)).await;
    }

    disp.parent.turn_off().await?;
    Ok(())
}
