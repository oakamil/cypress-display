// Copyright (c) 2026 Omair Kamil
// See LICENSE file in root directory for license terms.

use crate::{RotatedDisplay, Rotation, ssd1306::Ssd1306, ssd1351::Ssd1351};
use embedded_graphics::pixelcolor::{BinaryColor, Rgb565};
use std::error::Error;

pub enum TargetDisplay {
    Color(RotatedDisplay<Ssd1351, Rgb565>),
    Mono(RotatedDisplay<Ssd1306, BinaryColor>),
}

impl TargetDisplay {
    pub async fn turn_on(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            TargetDisplay::Color(d) => d.parent.turn_on().await,
            TargetDisplay::Mono(d) => d.parent.turn_on().await,
        }
    }

    pub async fn turn_off(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            TargetDisplay::Color(d) => d.parent.turn_off().await,
            TargetDisplay::Mono(d) => d.parent.turn_off().await,
        }
    }

    pub async fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            TargetDisplay::Color(d) => d.parent.flush().await,
            TargetDisplay::Mono(d) => d.parent.flush().await,
        }
    }

    pub async fn set_brightness(&mut self, brightness: u8) -> Result<(), Box<dyn Error>> {
        match self {
            TargetDisplay::Color(d) => d.parent.set_brightness(brightness).await,
            TargetDisplay::Mono(d) => d.parent.set_brightness(brightness).await,
        }
    }

    pub fn set_rotation(&mut self, rotation: Rotation) {
        match self {
            TargetDisplay::Color(d) => d.set_rotation(rotation),
            TargetDisplay::Mono(d) => d.set_rotation(rotation),
        }
    }

    pub fn clear(&mut self) {
        match self {
            TargetDisplay::Color(d) => d.clear(),
            TargetDisplay::Mono(d) => d.clear(),
        }
    }
}