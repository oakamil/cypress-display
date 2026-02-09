# Cypress Display

`cypress-display` provides a display controller designed to work alongside the [Cedar™](https://github.com/smroid/cedar) telescope control system. 

`cypress-display` drives a hardware OLED display, consuming data from Cedar™ server to display its PushTo guidance.

![Cypress Demo Small](https://github.com/user-attachments/assets/1374c194-d611-47f5-b0d9-2200074225a2)

## Hardware Requirements

This software is intended to run on Linux-based embedded hardware, specifically the **Raspberry Pi**, as it utilizes hardware-specific HALs (e.g. `rppal`, `linux-embedded-hal`).

### RGB Display

`cypress-display` can be used with an SSD1351-based OLED RGB display with a resolution of 128x128. It has been tested with the [Waveshare 1.5 RGB OLED display module](https://www.waveshare.com/1.5inch-rgb-oled-module.htm).
* **Interface**: SPI (must be enabled via raspi-config)
* **Wiring**: Refer to Waveshare's wiring [diagram](https://www.waveshare.com/img/devkit/LCD/1.5inch-RGB-OLED-Module/1.5inch-RGB-OLED-Module-details-5.jpg).

### White OLED Display

`cypress-display` can also be used with an SSD1306-based white (binary) OLED display, with a resolution of 128x64 (available in 0.96" size) or 128x32 (available in 0.91" size). It has been tested with [this generic OLED module](https://www.makerfocus.com/products/2pcs-i2c-oled-display-module-0-91-inch-i2c-ssd1306-oled-display-module-1?variant=31333400608845).
* **Interface**: I2C (must be enabled via raspi-config)
* **Wiring**: Connect VCC on the display to pin 1 on the Raspberry Pi, GND to pin 6, DIN to pin 3, and CLK to pin 5.

Red film is recommended to preserve night adaptation. Note that white OLED displays are typically much brighter than RGB displays at the lowest brightness settings, so multiple layers of film are advised to reduce the brightness.

## Building

### Software Prerequisites

* **Rust**: Stable toolchain (edition 2024).
* **Cedar™ Server**: This application expects `cedar-server` to be running on `localhost:80`.

### Build Instructions

You can build the project using the provided build script.

```Bash
./build.sh
```

This will place the binary and the web content into the directory `out/cypress/bin`.

## Usage

### cypress-display

The display driver daemon. It connects to the hardware display and queries Cedar™ server to render the UI.

```Bash
cd out/cypress/bin
./cypress-display --brightness 128
```
* `--brightness`: (Optional) Set physical display brightness (1-255). Default is 128 (50%).
* `--rotate`: (Optional) Set physical display clockwise rotation (0, 90, 180, or 270). Default is 0.
* `--type`: (Optional) Set physical display type. 1 = RGB 128x128, 2 = Binary 128x64, 3 = Binary 128x32. Default is 1.
* `--mirror`: (Optional) Mirror the physical display to the web UI.
* `--test`: (Optional) Test mode that cycles through various guidance values.

### Brightness and Rotation Control

The brightness and rotation can be updated in the field by connecting to the e-finder's WiFi network and accessing `cypress-display`'s control page at `https://192.168.4.1:6030`.

<img width="209" height="195" alt="cypress-control" src="https://github.com/user-attachments/assets/62f27993-ff80-49a5-b918-38d10ef4caed" />

### Display Mirror

`cypress-display` includes the ability to mirror the displayed output to the web UI.

```Bash
./cypress-display.sh --mirror
```

The mirrored display is available at `https://192.168.4.1:6030/mirror`. The mirrored display can be used without the presence of a physical screen as long as SPI is enabled on the e-finder device.

## Installation

A distribution zipfile is provided [here](https://github.com/oakamil/cypress-display/raw/refs/heads/main/dist/cypress-display.zip).

### Download Instructions

If your Cedar™ e-finder device has internet access the distribution archive can be downloaded directly:

```Bash
wget https://github.com/oakamil/cypress-display/raw/refs/heads/main/dist/cypress-display.zip
```

Otherwise you can download the file to a computer and use scp to transfer it to the e-finder device after connecting to its WiFi network:

```Bash
scp ~/Downloads/cypress-display.zip cedar@192.168.4.1:.
```

### Enabling SPI

SPI must be enabled on the e-finder device to use an RGB screen.

```Bash
sudo raspi-config
```

1. Select `3 Interface Options`
2. Select `I4 SPI`
3. Respond `<Yes>` to enable SPI
4. Continue by selecting `<OK>`
5. Select `<Finish>` to exit

### Enabling I2C

I2C must be enabled on the e-finder device to use a binary screen.

```Bash
sudo raspi-config
```

1. Select `3 Interface Options`
2. Select `I5 I2C`
3. Respond `<Yes>` to enable I2C
4. Continue by selecting `<OK>`
5. Select `<Finish>` to exit

### Install Script

The provided distribution archive includes a script to install `cypress-display` as a service to automatically start when the e-finder boots.

```Bash
unzip cypress-display.zip
./install.sh
```

The installation will set up `cypress-display` for use with a 128x128 RGB display. To use a different display update `cypress-display.service` to add the `--type 2` or `--type 3` argument to the end of `ExecStart`.

## License

This project is licensed under the Functional Source License, Version 1.1, MIT Future License (FSL-1.1-MIT).

See LICENSE.md for full details.

## Disclaimer

All product names, trademarks and registered trademarks are property of their respective owners. All company, product and service names used in this website are for identification purposes only. Use of these names, trademarks and brands does not imply endorsement.

`cypress-display` is not affiliated with, endorsed by, or sponsored by Clear Skies Astro.

Cedar™ is a trademark of Clear Skies Astro, registered in the U.S. and other countries.
