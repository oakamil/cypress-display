// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

use crate::prefs::{AppPrefs, save_brightness};
use axum::{
    Router,
    body::Bytes,
    extract::{Json, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{DrawTarget, OriginDimensions, Pixel, RgbColor, Size},
};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU8, Ordering},
};
use tower_http::services::ServeDir;

const SERVER_ADDRESS: &str = "0.0.0.0:6030";

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

pub fn start_server(ctx: ServerContext) -> Result<(), Box<dyn std::error::Error>> {
    let web_path = std::env::current_dir().unwrap_or_default().join("web");
    if !web_path.exists() {
        Err(format!(
            "Web directory not found at: {}",
            web_path.to_str().unwrap()
        ))?;
    }

    tokio::spawn(async move {
        let app = Router::new()
            .route("/api/brightness", get(get_brightness).post(set_brightness))
            .route("/api/frame", get(get_frame))
            .nest_service("/", ServeDir::new(web_path))
            .with_state(ctx);

        if let Ok(listener) = tokio::net::TcpListener::bind(SERVER_ADDRESS).await {
            println!("Web control UI running at http://{}", SERVER_ADDRESS);
            let _ = axum::serve(listener, app).await;
        } else {
            eprintln!("Failed to bind to {}", SERVER_ADDRESS);
        }
    });

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
        save_brightness(b);
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
