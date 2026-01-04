// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

mod cedar_client;
mod renderer;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        RwLock, // Added RwLock for shared frame data
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Bytes, // Use Bytes for efficient response
    extract::{Json, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use cedar_client::{CedarClient, ResponseStatus, ServerMode, ServerState};
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use linux_embedded_hal::Delay;
use renderer::{BG_COLOR, DrawState, draw_ui};
use rppal::{
    gpio::Gpio,
    spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi},
};
use serde::{Deserialize, Serialize};
use simple_signal::{self, Signal};
use ssd1351::display::display::Ssd1351;
use tokio::time::sleep;
use tower_http::services::ServeDir;

const PREFS_FILENAME: &str = "cb_prefs.json";
const SERVER_ADDRESS: &str = "0.0.0.0:6030";

#[derive(Serialize, Deserialize, Default, Clone)]
struct AppPrefs {
    brightness: Option<u8>,
}

#[derive(Clone)]
struct ServerContext {
    brightness: Arc<AtomicU8>,
    // Shared buffer for the latest frame (raw RGB565 bytes)
    frame: Arc<RwLock<Vec<u8>>>,
}

struct Framebuffer {
    pub pixels: [Rgb565; 128 * 128],
}

impl Framebuffer {
    fn new() -> Self {
        Self {
            pixels: [Rgb565::BLACK; 128 * 128],
        }
    }

    fn clear(&mut self, color: Rgb565) {
        self.pixels.fill(color);
    }

    // Helper to get raw bytes for the web stream
    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.pixels.as_ptr() as *const u8,
                self.pixels.len() * 2, // 2 bytes per pixel
            )
        }
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(128, 128)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if point.x >= 0 && point.x < 128 && point.y >= 0 && point.y < 128 {
                let index = (point.y as usize) * 128 + (point.x as usize);
                self.pixels[index] = color;
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    let cli_brightness = match args.opt_value_from_str::<_, u32>("--brightness")? {
        Some(val) if (1..=255).contains(&val) => Some(val as u8),
        Some(_) => return Err("Brightness must be between 1 and 255".into()),
        None => None,
    };

    let prefs_path = get_prefs_path()?;
    let file_brightness = if let Ok(contents) = std::fs::read_to_string(&prefs_path) {
        serde_json::from_str::<AppPrefs>(&contents)
            .ok()
            .and_then(|p| p.brightness)
            .unwrap_or(0x80)
    } else {
        0x80
    };

    let initial_brightness = cli_brightness.unwrap_or(file_brightness);

    let web_path = std::env::current_dir().unwrap_or_default().join("web");
    if !web_path.exists() {
        Err(format!(
            "Web directory not found at: {}",
            web_path.to_str().unwrap()
        ))?;
    }

    let shared_brightness = Arc::new(AtomicU8::new(initial_brightness));
    // Initialize shared frame with black pixels (128*128*2 bytes)
    let shared_frame = Arc::new(RwLock::new(vec![0u8; 128 * 128 * 2]));

    let server_ctx = ServerContext {
        brightness: shared_brightness.clone(),
        frame: shared_frame.clone(),
    };

    tokio::spawn(async move {
        let app = Router::new()
            .route("/api/brightness", get(get_brightness).post(set_brightness))
            .route("/api/frame", get(get_frame)) // New route for frame data
            .nest_service("/", ServeDir::new(web_path))
            .with_state(server_ctx);

        if let Ok(listener) = tokio::net::TcpListener::bind(SERVER_ADDRESS).await {
            println!("Web control UI running at http://{}", SERVER_ADDRESS);
            let _ = axum::serve(listener, app).await;
        } else {
            eprintln!("Failed to bind to {}", SERVER_ADDRESS);
        }
    });

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
    let mut disp = Ssd1351::new(spii);

    disp.reset(&mut rst, &mut Delay).unwrap();
    disp.turn_on().unwrap();

    let mut current_brightness = initial_brightness;
    disp.set_brightness(current_brightness).unwrap();

    // Virtual framebuffer for web rendering
    let mut web_fb = Framebuffer::new();

    let mut client = CedarClient::new();
    let mut last_slew: Option<ServerState> = None;
    let mut stale_angle = 0;

    while running.load(Ordering::SeqCst) {
        let target_brightness = shared_brightness.load(Ordering::Relaxed);
        if target_brightness != current_brightness {
            println!("Updating display brightness to {}", target_brightness);
            disp.set_brightness(target_brightness).unwrap();
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
        let _ = disp.flush();

        // Draw to virtual framebuffer and update shared state
        web_fb.clear(BG_COLOR);
        draw_ui(&mut web_fb, &draw_state);

        if let Ok(mut lock) = shared_frame.write() {
            lock.copy_from_slice(web_fb.as_bytes());
        }

        sleep(Duration::from_millis(50)).await;
    }

    disp.reset(&mut rst, &mut Delay).unwrap();
    disp.turn_off().unwrap();
    Ok(())
}

async fn get_brightness(State(ctx): State<ServerContext>) -> Json<AppPrefs> {
    let b = ctx.brightness.load(Ordering::Relaxed);
    Json(AppPrefs {
        brightness: Some(b),
    })
}

async fn set_brightness(
    State(ctx): State<ServerContext>,
    Json(payload): Json<AppPrefs>,
) -> StatusCode {
    if let Some(b) = payload.brightness {
        ctx.brightness.store(b, Ordering::Relaxed);
        if let Ok(path) = get_prefs_path() {
            let prefs = AppPrefs {
                brightness: Some(b),
            };
            if let Ok(data) = serde_json::to_string_pretty(&prefs) {
                let _ = std::fs::write(path, data);
            }
        }
    }
    StatusCode::OK
}

// Handler to serve the latest frame buffer
async fn get_frame(State(ctx): State<ServerContext>) -> impl IntoResponse {
    let frame_data = {
        if let Ok(lock) = ctx.frame.read() {
            lock.clone()
        } else {
            vec![]
        }
    };

    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Bytes::from(frame_data),
    )
}

fn get_prefs_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = std::env::current_exe()?;
    path.pop();
    path.push(PREFS_FILENAME);
    Ok(path)
}
