// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

mod cedar_client;
mod prefs;
mod renderer;
mod web;

use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use cedar_client::{CedarClient, ResponseStatus, ServerMode, ServerState};
use display_interface_spi::SPIInterface;
use embedded_graphics::draw_target::DrawTarget;
use linux_embedded_hal::Delay;
use renderer::{BG_COLOR, DrawState, RotatedDisplay, Rotation, draw_ui};
use rppal::{
    gpio::Gpio,
    spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi},
};
use simple_signal::{self, Signal};
use ssd1351::display::display::Ssd1351;
use tokio::time::sleep;
use web::{Framebuffer, ServerContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    let mirror_enabled = args.contains("--mirror");

    let cli_brightness = match args.opt_value_from_str::<_, u32>("--brightness")? {
        Some(val) if (1..=255).contains(&val) => Some(val as u8),
        Some(_) => return Err("Brightness must be between 1 and 255".into()),
        None => None,
    };

    let cli_rotation = match args.opt_value_from_str::<_, u32>("--rotation")? {
        Some(val) if val == 0 => Rotation::Deg0,
        Some(val) if val == 0 => Rotation::Deg90,
        Some(val) if val == 0 => Rotation::Deg180,
        Some(val) if val == 0 => Rotation::Deg270,
        Some(_) => return Err("Rotation must be one of 0, 90, 180, or 270".into()),
        None => Rotation::Deg0,
    };

    let file_brightness = prefs::load_brightness();
    let initial_brightness = cli_brightness.unwrap_or(file_brightness);

    let shared_brightness = Arc::new(AtomicU8::new(initial_brightness));
    // Initialize shared frame with black pixels (128*128*2 bytes)
    let shared_frame = Arc::new(RwLock::new(vec![0u8; 128 * 128 * 2]));

    let server_ctx = ServerContext {
        brightness: shared_brightness.clone(),
        frame: shared_frame.clone(),
    };

    web::start_server(server_ctx)?;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    simple_signal::set_handler(&[Signal::Int, Signal::Term], move |signal_rec| {
        println!("Signal received : '{:?}'", signal_rec);
        r.store(false, Ordering::SeqCst);
    });

    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 19660800, Mode::Mode0)?;
    let gpio = Gpio::new()?;
    let dc = gpio.get(25)?.into_output();
    let mut rst = gpio.get(27)?.into_output();

    let spii = SPIInterface::new(SimpleHalSpiDevice::new(spi), dc);
    let raw_disp = Ssd1351::new(spii);
    let mut disp = RotatedDisplay::new(raw_disp, cli_rotation);

    disp.parent.reset(&mut rst, &mut Delay).unwrap();
    disp.parent.turn_on().unwrap();

    let mut current_brightness = initial_brightness;
    disp.parent.set_brightness(current_brightness).unwrap();

    // Virtual framebuffer for web rendering
    let mut web_fb = if mirror_enabled {
        Some(Framebuffer::new())
    } else {
        None
    };

    let mut client = CedarClient::new();
    let mut last_slew: Option<ServerState> = None;
    let mut stale_angle = 0;

    while running.load(Ordering::SeqCst) {
        let target_brightness = shared_brightness.load(Ordering::Relaxed);
        if target_brightness != current_brightness {
            println!("Updating display brightness to {}", target_brightness);
            disp.parent.set_brightness(target_brightness).unwrap();
            current_brightness = target_brightness;
        }

        let resp = client.get_state().await;
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
        disp.clear(BG_COLOR).unwrap();
        draw_ui(&mut disp, &draw_state);
        let _ = disp.parent.flush();

        // Draw to virtual framebuffer
        if mirror_enabled {
            if let Some(fb) = &mut web_fb {
                fb.clear(BG_COLOR);
                draw_ui(fb, &draw_state);

                if let Ok(mut lock) = shared_frame.write() {
                    lock.copy_from_slice(fb.as_bytes());
                }
            }
        }

        sleep(Duration::from_millis(50)).await;
    }

    disp.parent.reset(&mut rst, &mut Delay).unwrap();
    disp.parent.turn_off().unwrap();
    Ok(())
}
