// Copyright (c) 2026 Omair Kamil
// See LICENSE file in root directory for license terms.

use crate::display::TargetDisplay;

use async_trait::async_trait;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use embedded_graphics_framebuf::FrameBuf;
use linux_embedded_hal::Delay;
use mipidsi::{Builder, Display, interface::SpiInterface, models::ST7789, options::ColorInversion};
use rppal::{
    gpio::{Gpio, OutputPin},
    pwm::{Channel, Polarity, Pwm},
    spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi},
};
use std::error::Error;

const WIDTH: usize = 135;
const HEIGHT: usize = 240;
const PIXEL_COUNT: usize = WIDTH * HEIGHT;

pub struct St7789 {
    display: Display<SpiInterface<'static, SimpleHalSpiDevice, OutputPin>, ST7789, OutputPin>,
    // Boxed fixed-size array for the in-memory framebuffer
    buffer: Box<[Rgb565; PIXEL_COUNT]>,
    bl: Pwm,
}

impl St7789 {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 160_000_000, Mode::Mode0)?;

        let gpio = Gpio::new()?;
        let dc = gpio.get(25)?.into_output();
        let rst = gpio.get(27)?.into_output();

        let bl = Pwm::with_frequency(Channel::Pwm0, 100.0, 0.0, Polarity::Normal, false)?;

        // mipidsi 0.10.0+ internal SPI batching buffer
        let spi_buffer = vec![0u8; 1024];
        let spi_buffer_slice: &'static mut [u8] = Box::leak(spi_buffer.into_boxed_slice());

        let spii = SpiInterface::new(SimpleHalSpiDevice::new(spi), dc, spi_buffer_slice);

        let mut display = Builder::new(ST7789, spii)
            .reset_pin(rst)
            .display_size(WIDTH as u16, HEIGHT as u16)
            .display_offset(52, 40)
            .invert_colors(ColorInversion::Inverted)
            .init(&mut Delay)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;

        // Allocate the main rendering framebuffer on the heap
        let vec_buffer = vec![Rgb565::BLACK; PIXEL_COUNT];
        let buffer = vec_buffer
            .into_boxed_slice()
            .try_into()
            .map_err(|_| "Failed to convert buffer to fixed-size array")?;

        display
            .clear(Rgb565::BLACK)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;

        Ok(Self {
            display,
            buffer,
            bl,
        })
    }
}

#[async_trait]
impl TargetDisplay for St7789 {
    async fn turn_on(&mut self) -> Result<(), Box<dyn Error>> {
        self.bl
            .set_duty_cycle(0.5)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;

        self.bl
            .enable()
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }

    async fn turn_off(&mut self) -> Result<(), Box<dyn Error>> {
        self.bl
            .set_duty_cycle(0.0)
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;

        self.bl
            .disable()
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }

    async fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        let screen_area = Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32));
        self.display
            .fill_contiguous(&screen_area, self.buffer.iter().copied())
            .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
    }

    async fn set_brightness(&mut self, brightness: u8) -> Result<(), Box<dyn Error>> {
        let duty_cycle = (brightness as f64) / 255.0;

        self.bl
            .set_duty_cycle(duty_cycle)
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

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut fb = FrameBuf::new(&mut *self.buffer, WIDTH, HEIGHT);
        fb.fill_contiguous(area, colors)
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
