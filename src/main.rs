// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

mod cedar_client;

use std::{
    path::PathBuf,
    sync::{
        Arc, LazyLock,
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
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Arc as DisplayArc, Line, PrimitiveStyle, Triangle},
};
use linux_embedded_hal::Delay;
use rppal::{
    gpio::Gpio,
    spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi},
};
use serde::{Deserialize, Serialize};
use simple_signal::{self, Signal};
use ssd1351::display::display::Ssd1351;
use tokio::time::sleep;
use tower_http::services::ServeDir;
use u8g2_fonts::{
    FontRenderer, fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
};

static STATUS_FONT: LazyLock<FontRenderer> =
    LazyLock::new(FontRenderer::new::<fonts::u8g2_font_logisoso16_tr>);

static GUIDANCE_FONT: LazyLock<FontRenderer> =
    LazyLock::new(FontRenderer::new::<fonts::u8g2_font_logisoso34_tr>);

const FG_COLOR: Rgb565 = Rgb565::RED;
const BG_COLOR: Rgb565 = Rgb565::BLACK;

const TRIANGLE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(FG_COLOR);
const TRIANGLE_STALE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(FG_COLOR, 2);
const ARROW_SHAFT_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(FG_COLOR, 3);
const ARROW_HEAD_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(FG_COLOR);
const ARC_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(FG_COLOR, 3);

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    // Command-line brightness takes precedence over the value in prefs
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
        // Default to 50%
        0x80
    };

    let initial_brightness = cli_brightness.unwrap_or(file_brightness);

    // Shared state for the web server and display loop
    let shared_brightness = Arc::new(AtomicU8::new(initial_brightness));
    let server_ctx = ServerContext {
        brightness: shared_brightness.clone(),
    };

    tokio::spawn(async move {
        let app = Router::new()
            .route("/api/brightness", get(get_brightness).post(set_brightness))
            .nest_service("/", ServeDir::new("web"))
            .with_state(server_ctx);

        if let Ok(listener) = tokio::net::TcpListener::bind(SERVER_ADDRESS).await {
            println!("Web UI running on http://{}", SERVER_ADDRESS);
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

        // Clear display for new frame
        disp.clear(BG_COLOR).unwrap();

        if resp.status != ResponseStatus::Success {
            STATUS_FONT
                .render_aligned(
                    format!("{:?}", resp.status).as_str(),
                    Point::new(64, 64),
                    VerticalPosition::Center,
                    HorizontalAlignment::Center,
                    FontColor::Transparent(FG_COLOR),
                    &mut disp,
                )
                .unwrap();
        } else if let Some(state) = resp.server_state {
            match state.server_mode {
                ServerMode::Operating => {
                    if !state.has_slew_request {
                        if state.has_solution {
                            last_slew = None;
                        }
                        if let Some(slew) = last_slew.clone() {
                            draw_operating_state(&mut disp, &slew, Some(stale_angle));
                            stale_angle = (stale_angle + 9) % 360;
                        } else {
                            STATUS_FONT
                                .render_aligned(
                                    "No Target",
                                    Point::new(64, 64),
                                    VerticalPosition::Center,
                                    HorizontalAlignment::Center,
                                    FontColor::Transparent(FG_COLOR),
                                    &mut disp,
                                )
                                .unwrap();
                        }
                    } else {
                        draw_operating_state(&mut disp, &state, None);
                        last_slew = Some(state.clone());
                    }
                }
                ServerMode::Calibrating => {
                    STATUS_FONT
                        .render_aligned(
                            "Calibrating",
                            Point::new(64, 64),
                            VerticalPosition::Center,
                            HorizontalAlignment::Center,
                            FontColor::Transparent(FG_COLOR),
                            &mut disp,
                        )
                        .unwrap();
                }
                _ => {
                    STATUS_FONT
                        .render_aligned(
                            "Setup Mode",
                            Point::new(64, 64),
                            VerticalPosition::Center,
                            HorizontalAlignment::Center,
                            FontColor::Transparent(FG_COLOR),
                            &mut disp,
                        )
                        .unwrap();
                }
            }
        }

        let _ = disp.flush();
        sleep(Duration::from_millis(50)).await;
    }

    // Cleanup on exit
    disp.reset(&mut rst, &mut Delay).unwrap();
    disp.turn_off().unwrap();
    Ok(())
}

fn draw_operating_state<D>(disp: &mut D, state: &ServerState, stale_angle: Option<u32>)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: std::fmt::Debug,
{
    let is_current = stale_angle.is_some();
    let tilt = state.tilt_target_distance;
    let rot = state.rotation_target_distance;

    GUIDANCE_FONT
        .render_aligned(
            format_offset(tilt).as_str(),
            Point::new(127, 0),
            VerticalPosition::Top,
            HorizontalAlignment::Right,
            FontColor::Transparent(FG_COLOR),
            disp,
        )
        .unwrap();

    GUIDANCE_FONT
        .render_aligned(
            format_offset(rot).as_str(),
            Point::new(127, 127),
            VerticalPosition::Baseline,
            HorizontalAlignment::Right,
            FontColor::Transparent(FG_COLOR),
            disp,
        )
        .unwrap();

    if !state.is_alt_az {
        if is_current || (stale_angle.unwrap() % 72 < 36) {
            GUIDANCE_FONT
                .render_aligned(
                    if tilt > 0.0 { "N" } else { "S" },
                    Point::new(0, 0),
                    VerticalPosition::Top,
                    HorizontalAlignment::Left,
                    FontColor::Transparent(FG_COLOR),
                    disp,
                )
                .unwrap();

            GUIDANCE_FONT
                .render_aligned(
                    if rot > 0.0 { "E" } else { "W" },
                    Point::new(0, 127),
                    VerticalPosition::Baseline,
                    HorizontalAlignment::Left,
                    FontColor::Transparent(FG_COLOR),
                    disp,
                )
                .unwrap();
        }
    } else {
        let tri_style = if is_current {
            TRIANGLE_STYLE
        } else {
            TRIANGLE_STALE_STYLE
        };
        if tilt > 0.0 {
            Triangle::new(Point::new(15, 0), Point::new(0, 30), Point::new(30, 30))
        } else {
            Triangle::new(Point::new(0, 0), Point::new(30, 0), Point::new(15, 30))
        }
        .into_styled(tri_style)
        .draw(disp)
        .unwrap();

        if rot > 0.0 {
            Triangle::new(Point::new(0, 97), Point::new(0, 127), Point::new(30, 112))
        } else {
            Triangle::new(Point::new(30, 97), Point::new(30, 127), Point::new(0, 112))
        }
        .into_styled(tri_style)
        .draw(disp)
        .unwrap();
    }

    if !is_current {
        DisplayArc::new(
            Point::new(44, 44),
            40,
            (stale_angle.unwrap() as f32).deg(),
            90.0.deg(),
        )
        .into_styled(ARC_STYLE)
        .draw(disp)
        .unwrap();
        return;
    }

    let display_angle_rad = (state.target_angle as f64 + 90.0).to_radians();

    let total_len = 40.0;
    let half_len = total_len / 2.0;
    let head_len = 12.0;
    let head_width = 12.0;

    let cos_a = display_angle_rad.cos();
    let sin_a = display_angle_rad.sin();

    let tip = Point::new(
        64 + (half_len * cos_a) as i32,
        64 - (half_len * sin_a) as i32,
    );

    let tail = Point::new(
        64 - (half_len * cos_a) as i32,
        64 + (half_len * sin_a) as i32,
    );

    let head_base_offset = half_len - head_len;
    let head_base_center = Point::new(
        64 + (head_base_offset * cos_a) as i32,
        64 - (head_base_offset * sin_a) as i32,
    );

    let angle_perp_plus = display_angle_rad + std::f64::consts::FRAC_PI_2;
    let angle_perp_minus = display_angle_rad - std::f64::consts::FRAC_PI_2;
    let half_width = head_width / 2.0;

    let corner1 = Point::new(
        head_base_center.x + (half_width * angle_perp_plus.cos()) as i32,
        head_base_center.y - (half_width * angle_perp_plus.sin()) as i32,
    );

    let corner2 = Point::new(
        head_base_center.x + (half_width * angle_perp_minus.cos()) as i32,
        head_base_center.y - (half_width * angle_perp_minus.sin()) as i32,
    );

    Line::new(tail, head_base_center)
        .into_styled(ARROW_SHAFT_STYLE)
        .draw(disp)
        .unwrap();

    Triangle::new(tip, corner1, corner2)
        .into_styled(ARROW_HEAD_STYLE)
        .draw(disp)
        .unwrap();
}

fn format_offset(num: f64) -> String {
    let n = num.abs();
    if n >= 100.0 {
        format!("{:.0}", n)
    } else if n >= 10.0 {
        format!("{:.1}", n)
    } else {
        format!("{:.2}", n)
    }
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
