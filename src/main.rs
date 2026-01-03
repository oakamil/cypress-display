// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
    time::Duration,
};

use cypress_display::cedar_client::{
    CedarClient, ResponseStatus, ServerMode, ServerState,
};
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
use simple_signal::{self, Signal};
use ssd1351::display::display::Ssd1351;
use tokio::time::sleep;
use u8g2_fonts::{
    fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    FontRenderer,
};

static STATUS_FONT: LazyLock<FontRenderer> =
    LazyLock::new(FontRenderer::new::<fonts::u8g2_font_logisoso16_tr>);

static GUIDANCE_FONT: LazyLock<FontRenderer> =
    LazyLock::new(FontRenderer::new::<fonts::u8g2_font_logisoso34_tr>);

const FG_COLOR: Rgb565 = Rgb565::RED;
const BG_COLOR: Rgb565 = Rgb565::BLACK;

const TRIANGLE_STYLE: PrimitiveStyle<Rgb565> =
    PrimitiveStyle::with_fill(FG_COLOR);
const TRIANGLE_STALE_STYLE: PrimitiveStyle<Rgb565> =
    PrimitiveStyle::with_stroke(FG_COLOR, 2);
const ARROW_SHAFT_STYLE: PrimitiveStyle<Rgb565> =
    PrimitiveStyle::with_stroke(FG_COLOR, 3);
const ARROW_HEAD_STYLE: PrimitiveStyle<Rgb565> =
    PrimitiveStyle::with_fill(FG_COLOR);
const ARC_STYLE: PrimitiveStyle<Rgb565> =
    PrimitiveStyle::with_stroke(FG_COLOR, 3);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();
    let brightness: u8 = match args
        .opt_value_from_str::<_, u32>("--brightness")?
    {
        Some(val) if (1..=255).contains(&val) => val as u8,
        Some(_) => return Err("Brightness must be between 1 and 255".into()),
        None => 0x80, // Default to 50%
    };

    // If the program is terminated make sure we can clean up
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    simple_signal::set_handler(
        &[Signal::Int, Signal::Term],
        move |signal_rec| {
            println!("Signal received : '{:?}'", signal_rec);
            r.store(false, Ordering::SeqCst);
        },
    );

    // Initialize the OLED display (using hardware configuration from
    // cedar-lite-server)
    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 19660800, Mode::Mode0)?;
    let gpio = Gpio::new()?;
    let dc = gpio.get(25)?.into_output();
    let mut rst = gpio.get(27)?.into_output();

    let spii = SPIInterface::new(SimpleHalSpiDevice::new(spi), dc);
    let mut disp = Ssd1351::new(spii);

    disp.reset(&mut rst, &mut Delay).unwrap();
    disp.turn_on().unwrap();
    // Set the brightness to 50%
    disp.set_brightness(brightness).unwrap();

    let mut client = CedarClient::new();

    // Keep the last valid guidance to display while slewing
    let mut last_slew: Option<ServerState> = None;
    let mut stale_angle = 0;

    while running.load(Ordering::SeqCst) {
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
                            draw_operating_state(&mut disp, &slew, false, stale_angle);
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
                        draw_operating_state(&mut disp, &state, true, 0);
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

fn draw_operating_state<D>(
    disp: &mut D,
    state: &ServerState,
    is_current: bool,
    stale_angle: u32,
) where
    D: DrawTarget<Color = Rgb565>,
    D::Error: std::fmt::Debug,
{
    let tilt = state.tilt_target_distance;
    let rot = state.rotation_target_distance;

    // Draw the tilt offset at the top
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

    // Draw the rotation offset at the bottom
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

    // For EQ render the cardinal direction label
    if !state.is_alt_az {
        // Blink the label if stale
        if is_current || (stale_angle % 72 < 36) {
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
        // Render direction triangles for alt-az
        let tri_style = if is_current {
            TRIANGLE_STYLE
        } else {
            TRIANGLE_STALE_STYLE
        };
        if tilt > 0.0 {
            Triangle::new(
                Point::new(15, 0),
                Point::new(0, 30),
                Point::new(30, 30),
            )
        } else {
            Triangle::new(
                Point::new(0, 0),
                Point::new(30, 0),
                Point::new(15, 30),
            )
        }
        .into_styled(tri_style)
        .draw(disp)
        .unwrap();

        if rot > 0.0 {
            Triangle::new(
                Point::new(0, 97),
                Point::new(0, 127),
                Point::new(30, 112),
            )
        } else {
            Triangle::new(
                Point::new(30, 97),
                Point::new(30, 127),
                Point::new(0, 112),
            )
        }
        .into_styled(tri_style)
        .draw(disp)
        .unwrap();
    }

    if !is_current {
        // Solution is not fresh, indicate that to user
        // STATUS_FONT
        // .render_aligned(
        // "...",
        // Point::new(64, 64),
        // VerticalPosition::Center,
        // HorizontalAlignment::Center,
        // FontColor::Transparent(FG_COLOR),
        // disp,
        // )
        // .unwrap();

        // Render a quarter circle (90 degree sector) with radius 20
        // centered at (64, 64). The bounding box is (64-20, 64-20)
        // with a diameter of 40.
        DisplayArc::new(
            Point::new(44, 44),
            40,
            (stale_angle as f32).deg(),
            90.0.deg(),
        )
        .into_styled(ARC_STYLE)
        .draw(disp)
        .unwrap();
        return;
    }

    // Render target angle arrow

    // Angle math assumes that 0 is right, 90 is up. Cedar
    // uses 0 as up and 90 as left.
    let display_angle_rad = (state.target_angle as f64 + 90.0).to_radians();

    // Arrow Dimensions
    let total_len = 40.0;
    // 20.0px offset from center for tip and tail
    let half_len = total_len / 2.0;
    let head_len = 12.0;
    let head_width = 12.0;

    // Calculate tip and tail points relative to the screen
    // center
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

    // The head base center is 'head_len' back from the tip
    let head_base_offset = half_len - head_len;
    let head_base_center = Point::new(
        64 + (head_base_offset * cos_a) as i32,
        64 - (head_base_offset * sin_a) as i32,
    );

    // Perpendicular angles for the triangle base corners
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

    // Draw shaft from the tail to the base of the head
    Line::new(tail, head_base_center)
        .into_styled(ARROW_SHAFT_STYLE)
        .draw(disp)
        .unwrap();

    // Draw head
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
