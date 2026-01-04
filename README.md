# Cypress Display

`cypress-display` provides a display controller designed to work alongside the [Cedar™](https://github.com/smroid/cedar) telescope control system. 

`cypress-display` drives a hardware OLED display, consuming data from Cedar™ server to display its PushTo guidance.

## Hardware Requirements

This software is intended to run on Linux-based embedded hardware, specifically the **Raspberry Pi**, as it utilizes hardware-specific HALs (e.g. `rppal`, `linux-embedded-hal`).

`cypress-display` is built for use with an SSD1351-based OLED RGB display with a resolution of 128x128. It has been tested with the [Waveshare 1.5 RGB OLED display module](https://www.waveshare.com/1.5inch-rgb-oled-module.htm).
* **Interface**: SPI (must be enabled via raspi-config)
* **Wiring**: Refer to Waveshare's wiring [diagram](https://www.waveshare.com/img/devkit/LCD/1.5inch-RGB-OLED-Module/1.5inch-RGB-OLED-Module-details-5.jpg)

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
* `--brightness`: (Optional) Set display brightness (1-255). Default is 128 (50%).
* `--record`: (Optional) Record video of the output to the specified file.


### Brightness Update

The brightness can be updated in the field by connecting to the e-finder's WiFi network and accessing `cypress-display`'s control page at `https://192.168.4.1:6030`.

<img width="575" height="409" alt="cypress-display" src="https://github.com/user-attachments/assets/cfaa94f2-fa01-4663-88f6-4cc9952af6e1" />

### Recording Display

`cypress-display` includes the ability to record the displayed output to a file. FFmpeg must be installed on the system:

```Bash
sudo apt-get install ffmpeg -y

./cypress-display.sh --record /tmp/cypress.mp4
```

The recorded video will mirror the OLED display.

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

### Install Script

The provided distribution archive includes a script to install `cypress-display` as a service to automatically start when the e-finder boots.

```Bash
unzip cypress-display.zip
./install.sh
```

## License

This project is licensed under the Functional Source License, Version 1.1, MIT Future License (FSL-1.1-MIT).

See LICENSE.md for full details.

## Disclaimer

All product names, trademarks and registered trademarks are property of their respective owners. All company, product and service names used in this website are for identification purposes only. Use of these names, trademarks and brands does not imply endorsement.

`cypress-display` is not affiliated with, endorsed by, or sponsored by Clear Skies Astro.

Cedar™ is a trademark of Clear Skies Astro, registered in the U.S. and other countries.
