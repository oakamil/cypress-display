use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Arc as DisplayArc, Line, PrimitiveStyle, Triangle},
};
use std::sync::LazyLock;
use u8g2_fonts::{
    FontRenderer, fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
};

use crate::cedar_client::ServerState;

static STATUS_FONT: LazyLock<FontRenderer> =
    LazyLock::new(FontRenderer::new::<fonts::u8g2_font_logisoso16_tr>);

static GUIDANCE_FONT: LazyLock<FontRenderer> =
    LazyLock::new(FontRenderer::new::<fonts::u8g2_font_logisoso34_tr>);

pub const FG_COLOR: Rgb565 = Rgb565::RED;
pub const BG_COLOR: Rgb565 = Rgb565::BLACK;

const TRIANGLE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(FG_COLOR);
const TRIANGLE_STALE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(FG_COLOR, 2);
const ARROW_SHAFT_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(FG_COLOR, 3);
const ARROW_HEAD_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(FG_COLOR);
const ARC_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(FG_COLOR, 3);

// Represents the visual state of the screen
pub enum DrawState<'a> {
    Message(String),
    // State, stale_angle
    Operating(&'a ServerState, Option<u32>),
}

// Draw the UI to any target display
pub fn draw_ui<D>(target: &mut D, state: &DrawState)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: std::fmt::Debug,
{
    match state {
        DrawState::Message(msg) => {
            STATUS_FONT
                .render_aligned(
                    msg.as_str(),
                    Point::new(64, 64),
                    VerticalPosition::Center,
                    HorizontalAlignment::Center,
                    FontColor::Transparent(FG_COLOR),
                    target,
                )
                .unwrap();
        }
        DrawState::Operating(s, stale) => {
            draw_operating_state(target, s, *stale);
        }
    }
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
