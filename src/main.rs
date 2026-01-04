// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

mod cedar_client;
mod renderer;

use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    extract::{Json, State},
    http::StatusCode,
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
            // Check bounds to prevent panics
            if point.x >= 0 && point.x < 128 && point.y >= 0 && point.y < 128 {
                let index = (point.y as usize) * 128 + (point.x as usize);
                self.pixels[index] = color;
            }
        }
        Ok(())
    }
}

struct VideoRecorder {
    process: Child,
    fb: Framebuffer,
}

impl VideoRecorder {
    fn new(filename: &str) -> std::io::Result<Self> {
        // Spawns ffmpeg to read raw RGB565LE video from stdin
        let process = Command::new("ffmpeg")
            .args(&[
                // Overwrite output
                "-y",
                "-f",
                "rawvideo",
                // Little Endian RGB565 (RPi default)
                "-pixel_format",
                "rgb565le",
                "-video_size",
                "128x128",
                "-framerate",
                "20",
                // Read from stdin
                "-i",
                "-",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                filename,
            ])
            .stdin(Stdio::piped())
            .spawn()?;

        Ok(Self {
            process,
            fb: Framebuffer::new(),
        })
    }

    fn draw_and_write(&mut self, state: &DrawState) {
        self.fb.clear(BG_COLOR);

        // Draw the exact same content as the screen
        draw_ui(&mut self.fb, state);

        // Write raw bytes to ffmpeg
        if let Some(stdin) = self.process.stdin.as_mut() {
            // Unsafe cast: Treat the [Rgb565] array as a byte slice.
            // Rgb565 is a transparent wrapper around u16, so this is safe for reading.
            let ptr = self.fb.pixels.as_ptr() as *const u8;
            let len = self.fb.pixels.len() * 2; // 2 bytes per pixel
            let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
            let _ = stdin.write_all(bytes);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    // Command-line brightness takes precedence over the value in prefs
    let cli_brightness = match args.opt_value_from_str::<_, u32>("--brightness")? {
        Some(val) if (1..=255).contains(&val) => Some(val as u8),
        Some(_) => return Err("Brightness must be between 1 and 255".into()),
        None => None,
    };

    // Record video of the displayed screen to the specified file if requested
    let record_file = args.opt_value_from_str::<_, String>("--record")?;

    let prefs_path = get_prefs_path()?;
    let file_brightness = if let Ok(contents) = std::fs::read_to_string(&prefs_path) {
        serde_json::from_str::<AppPrefs>(&contents)
            .ok()
            .and_then(|p| p.brightness)
            .unwrap_or(0x80)
    } else {
        // Default to 50%
        0x80
    };

    let initial_brightness = cli_brightness.unwrap_or(file_brightness);

    // Check web assets path
    let web_path = std::env::current_dir().unwrap_or_default().join("web");
    if !web_path.exists() {
        Err(format!(
            "Web directory not found at: {}",
            web_path.to_str().unwrap()
        ))?;
    }

    // Shared state for the web server and display loop
    let shared_brightness = Arc::new(AtomicU8::new(initial_brightness));
    let server_ctx = ServerContext {
        brightness: shared_brightness.clone(),
    };

    tokio::spawn(async move {
        let app = Router::new()
            .route("/api/brightness", get(get_brightness).post(set_brightness))
            .nest_service("/", ServeDir::new(web_path))
            .with_state(server_ctx);

        if let Ok(listener) = tokio::net::TcpListener::bind(SERVER_ADDRESS).await {
            println!("Brightness control UI running at http://{}", SERVER_ADDRESS);
            let _ = axum::serve(listener, app).await;
        } else {
            eprintln!("Failed to bind to {}", SERVER_ADDRESS);
        }
    });

    // If the program is terminated make sure we can clean up
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    simple_signal::set_handler(&[Signal::Int, Signal::Term], move |signal_rec| {
        println!("Signal received : '{:?}'", signal_rec);
        r.store(false, Ordering::SeqCst);
    });

    // Initialize the OLED display
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

    // Initialize the video recorder if requested
    let mut recorder = if let Some(filename) = record_file {
        println!("Recording video to: {}", filename);
        Some(VideoRecorder::new(&filename)?)
    } else {
        None
    };

    let mut client = CedarClient::new();

    // Keep the last valid guidance to display while slewing
    let mut last_slew: Option<ServerState> = None;
    let mut stale_angle = 0;

    while running.load(Ordering::SeqCst) {
        // Check if brightness changed via the web UI
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

        // Clear display for new frame
        disp.clear(BG_COLOR).unwrap();
        draw_ui(&mut disp, &draw_state);
        let _ = disp.flush();

        if let Some(rec) = &mut recorder {
            rec.draw_and_write(&draw_state);
        }

        sleep(Duration::from_millis(50)).await;
    }

    // Cleanup on exit
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

        // Save to prefs
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

fn get_prefs_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = std::env::current_exe()?;
    path.pop();
    path.push(PREFS_FILENAME);
    Ok(path)
}
