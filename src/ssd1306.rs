// Copyright (c) 2026 Omair Kamil
// See LICENSE file in root directory for license terms.

use crate::display::TargetDisplay;

use async_trait::async_trait;
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use rppal::i2c::I2c;
use ssd1306::{
    I2CDisplayInterface, Ssd1306 as Ssd1306Driver, mode::BufferedGraphicsMode, prelude::*,
};
use std::error::Error;

type Interface = I2CInterface<I2c>;
type Driver32 =
    Ssd1306Driver<Interface, DisplaySize128x32, BufferedGraphicsMode<DisplaySize128x32>>;
type Driver64 =
    Ssd1306Driver<Interface, DisplaySize128x64, BufferedGraphicsMode<DisplaySize128x64>>;

enum DriverVariant {
    Size128x32(Driver32),
    Size128x64(Driver64),
}

pub struct Ssd1306 {
    driver: DriverVariant,
}

impl Ssd1306 {
    pub fn new_128_32() -> Result<Self, Box<dyn Error>> {
        let i2c = I2c::new()?;
        let interface = I2CDisplayInterface::new(i2c);

        let driver = Ssd1306Driver::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();

        Ok(Self {
            driver: DriverVariant::Size128x32(driver),
        })
    }

    pub fn new_128_64() -> Result<Self, Box<dyn Error>> {
        let i2c = I2c::new()?;
        let interface = I2CDisplayInterface::new(i2c);

        let driver = Ssd1306Driver::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();

        Ok(Self {
            driver: DriverVariant::Size128x64(driver),
        })
    }
}

#[async_trait]
impl TargetDisplay for Ssd1306 {
    async fn turn_on(&mut self) -> Result<(), Box<dyn Error>> {
        match &mut self.driver {
            DriverVariant::Size128x32(d) => {
                d.init()
                    .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;
                d.set_display_on(true)
                    .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
            }
            DriverVariant::Size128x64(d) => {
                d.init()
                    .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))?;
                d.set_display_on(true)
                    .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e)))
            }
        }
    }

    async fn turn_off(&mut self) -> Result<(), Box<dyn Error>> {
        match &mut self.driver {
            DriverVariant::Size128x32(d) => d
                .set_display_on(false)
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
            DriverVariant::Size128x64(d) => d
                .set_display_on(false)
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
        }
    }

    async fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        match &mut self.driver {
            DriverVariant::Size128x32(d) => d
                .flush()
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
            DriverVariant::Size128x64(d) => d
                .flush()
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
        }
    }

    async fn set_brightness(&mut self, brightness: u8) -> Result<(), Box<dyn Error>> {
        match &mut self.driver {
            DriverVariant::Size128x32(d) => d
                .set_brightness(Brightness::custom(1, brightness))
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
            DriverVariant::Size128x64(d) => d
                .set_brightness(Brightness::custom(1, brightness))
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
        }
    }
}

impl DrawTarget for Ssd1306 {
    type Color = BinaryColor;
    type Error = Box<dyn Error>;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        match &mut self.driver {
            DriverVariant::Size128x32(d) => d
                .draw_iter(pixels)
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
            DriverVariant::Size128x64(d) => d
                .draw_iter(pixels)
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
        }
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        match &mut self.driver {
            DriverVariant::Size128x32(d) => d
                .clear(color)
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
            DriverVariant::Size128x64(d) => d
                .clear(color)
                .map_err(|e| Box::<dyn Error>::from(format!("{:?}", e))),
        }
    }
}

impl OriginDimensions for Ssd1306 {
    fn size(&self) -> Size {
        match &self.driver {
            DriverVariant::Size128x32(d) => d.size(),
            DriverVariant::Size128x64(d) => d.size(),
        }
    }
}
