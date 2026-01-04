// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

use axum::{
    body::Bytes,
    extract::{Json, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU8, Ordering},
    },
};

const PREFS_FILENAME: &str = "cb_prefs.json";

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct AppPrefs {
    pub brightness: Option<u8>,
}

#[derive(Clone)]
pub struct ServerContext {
    pub brightness: Arc<AtomicU8>,
    // Shared buffer for the latest frame (raw RGB565 bytes)
    pub frame: Arc<RwLock<Vec<u8>>>,
}

pub struct Framebuffer {
    pub pixels: [Rgb565; 128 * 128],
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: [Rgb565::BLACK; 128 * 128],
        }
    }

    pub fn clear(&mut self, color: Rgb565) {
        self.pixels.fill(color);
    }

    // Helper to get raw bytes for the web stream
    pub fn as_bytes(&self) -> &[u8] {
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

pub async fn get_brightness(State(ctx): State<ServerContext>) -> Json<AppPrefs> {
    let b = ctx.brightness.load(Ordering::Relaxed);
    Json(AppPrefs {
        brightness: Some(b),
    })
}

pub async fn set_brightness(
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
pub async fn get_frame(State(ctx): State<ServerContext>) -> impl IntoResponse {
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

pub fn get_prefs_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = std::env::current_exe()?;
    path.pop();
    path.push(PREFS_FILENAME);
    Ok(path)
}
