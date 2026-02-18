// Copyright (c) 2026 Omair Kamil
// See LICENSE file in root directory for license terms.

use crate::display::TargetDisplay;

use async_trait::async_trait;
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use embedded_graphics_framebuf::FrameBuf;
use linux_embedded_hal::Delay;
use rppal::{
    gpio::{Gpio, OutputPin},
    spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi},
};
use st7789::ST7789 as St7789Driver;
use std::error::Error;

const WIDTH: usize = 240;
const HEIGHT: usize = 240;
const PIXEL_COUNT: usize = WIDTH * HEIGHT;

pub struct St7789 {
    driver: St7789Driver<SPIInterface<SimpleHalSpiDevice, OutputPin>, OutputPin, OutputPin>,
    // Boxed fixed-size array to satisfy FrameBufferBackend trait
    buffer: Box<[Rgb565; PIXEL_COUNT]>,
    // Hold the BL pin directly to control PWM
    bl: OutputPin,
}

impl St7789 {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 160_000_000, Mode::Mode0)?;

        let gpio = Gpio::new()?;
        let dc = gpio.get(25)?.into_output();
        let rst = gpio.get(27)?.into_output();
        let bl = gpio.get(24)?.into_output();

        let spii = SPIInterface::new(SimpleHalSpiDevice::new(spi), dc);
        let mut driver = St7789Driver::new(spii, Some(rst), None, WIDTH as u16, HEIGHT as u16);

        driver
            .init(&mut Delay)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;

        // Allocate buffer on heap
        let vec_buffer = vec![Rgb565::BLACK; PIXEL_COUNT];
        let buffer = vec_buffer
            .into_boxed_slice()
            .try_into()
            .map_err(|_| "Failed to convert buffer to fixed-size array")?;

        Ok(Self { driver, buffer, bl })
    }
}

#[async_trait]
impl TargetDisplay for St7789 {
    async fn turn_on(&mut self) -> Result<(), Box<dyn Error>> {
        // Default to half brightness
        self.bl
            .set_pwm_frequency(100.0, 0.5)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }

    async fn turn_off(&mut self) -> Result<(), Box<dyn Error>> {
        // Turn off backlight
        self.bl
            .set_pwm_frequency(100.0, 0.0)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }

    async fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        let raw_pixels = self
            .buffer
            .iter()
            .map(|p| embedded_graphics::pixelcolor::raw::RawU16::from(*p).into_inner());

        self.driver
            .set_pixels(0, 0, (WIDTH - 1) as u16, (HEIGHT - 1) as u16, raw_pixels)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }

    async fn set_brightness(&mut self, brightness: u8) -> Result<(), Box<dyn Error>> {
        // Convert u8 (0-255) to f64 (0.0-1.0) and apply directly
        let duty_cycle = (brightness as f64) / 255.0;

        self.bl
            .set_pwm_frequency(100.0, duty_cycle)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }
}

impl DrawTarget for St7789 {
    type Color = Rgb565;
    type Error = Box<dyn Error>;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let mut fb = FrameBuf::new(&mut *self.buffer, WIDTH, HEIGHT);
        fb.draw_iter(pixels)
            .map_err(|_| Box::<dyn Error>::from("Framebuffer draw error"))
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let mut fb = FrameBuf::new(&mut *self.buffer, WIDTH, HEIGHT);
        fb.fill_solid(area, color)
            .map_err(|_| Box::<dyn Error>::from("Framebuffer fill error"))
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let mut fb = FrameBuf::new(&mut *self.buffer, WIDTH, HEIGHT);
        fb.clear(color)
            .map_err(|_| Box::<dyn Error>::from("Framebuffer clear error"))
    }
}

impl OriginDimensions for St7789 {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}
