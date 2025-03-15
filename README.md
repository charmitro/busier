# Busier - ESP32 Status Display and Web Server

A Rust application for ESP32 microcontrollers that creates a web interface for controlling your availability status, with real-time display on an SSD1306 OLED screen.

## Features

- 🌐 WiFi connectivity to your local network
- 🖥️ Built-in HTTP server with a responsive web interface
- 📱 Toggle between "Free" and "Do Not Disturb" status from any device
- 📊 OLED display showing real-time status, IP address, and request count
- 🛠️ Built entirely in Rust using the ESP-IDF framework

## Hardware Requirements

- ESP32 development board
- SSD1306 OLED display (128x32 or 128x64)
- I2C connection wires

## Wiring

Connect the SSD1306 OLED display to the ESP32:
- SDA to GPIO21
- SCL to GPIO22
- VCC to 3.3V
- GND to GND

## Building and Flashing

### Prerequisites

1. Install Rust and Cargo (https://rustup.rs/)
2. Install ESP-IDF toolchain using `espup` (https://github.com/esp-rs/espup)
3. Add the ESP32 target: `rustup target add xtensa-esp32-espidf`

### Building

1. Clone this repository:
   ```
   git clone https://github.com/charmitro/busier.git
   cd busier
   ```

2. Configure your WiFi credentials (use environment variables for security):
   ```
   export WIFI_SSID="your_wifi_name"
   export WIFI_PASS="your_wifi_password"
   ```

3. Build and flash:
   ```
   cargo build --release
   cargo espflash flash --release
   ```

4. Monitor the serial output (optional):
   ```
   cargo espflash monitor
   ```

## Usage

1. After the ESP32 boots, it will display the IP address on the OLED screen
2. Open a web browser and navigate to the displayed IP address
3. Use the web interface to toggle between "Free" and "Do Not Disturb" status
4. The OLED display will update to show the current status

## Project Structure

- `src/main.rs` - Main application code
- `src/http_server_page.html` - HTML template for the web interface
- `build.rs` - Build script for embedding environment variables
- `Cargo.toml` - Project dependencies and configuration

## Configuration

The project uses the following environment variables:
- `WIFI_SSID`: Your WiFi network name
- `WIFI_PASS`: Your WiFi password

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [esp-rs](https://github.com/esp-rs) - Rust support for ESP32
- [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics) - Graphics library for embedded displays
- [ssd1306](https://github.com/jamwaffles/ssd1306) - SSD1306 OLED driver
