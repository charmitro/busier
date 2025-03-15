//! ESP32 HTTP Server with WiFi Client and SSD1306 OLED display.
//!
//! Connects to existing WiFi network, serves a simple HTML page,
//! and displays information on an SSD1306 OLED display.
//! Includes a "Do Not Disturb" toggle button.

use core::convert::TryInto;
use embedded_svc::http::{Headers, Method};
use embedded_svc::io::Write;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};

use esp_idf_svc::hal::i2c;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::{Configuration as HttpConfiguration, EspHttpServer},
    nvs::EspDefaultNvsPartition,
};

use log::info;

// SSD1306 OLED display
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306, mode::BufferedGraphicsMode};

// Standard library
use std::sync::atomic::{AtomicBool, Ordering};

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");
static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>ESP32 Status Controller</title>
    <style>
        body { 
            font-family: Arial, sans-serif; 
            margin: 0; 
            padding: 20px; 
            text-align: center; 
            background-color: #f5f5f5;
        }
        h1 { 
            color: #333366; 
            margin-bottom: 30px;
        }
        .container { 
            max-width: 600px; 
            margin: 0 auto; 
            background-color: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }
        button { 
            background-color: #4CAF50; 
            color: white; 
            padding: 12px 25px; 
            border: none; 
            border-radius: 4px;
            cursor: pointer; 
            margin: 10px; 
            font-size: 16px;
            transition: all 0.3s;
        }
        button:hover {
            opacity: 0.9;
            transform: translateY(-2px);
        }
        .dnd-button { 
            background-color: #f44336; 
        }
        .free-button { 
            background-color: #4CAF50; 
        }
        .status-panel { 
            margin: 20px 0; 
            padding: 25px; 
            border: 1px solid #ddd; 
            border-radius: 5px;
            background-color: #fafafa;
        }
        .current-status { 
            font-weight: bold; 
            font-size: 1.4em;
            display: block;
            margin: 10px 0 20px 0;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>ESP32 Status Controller</h1>
        
        <div class="status-panel">
            <p>Current Status:</p>
            <span id="current-status" class="current-status">Loading...</span>
            <div>
                <button id="dnd-button" class="dnd-button" onclick="setStatus('dnd')">Do Not Disturb</button>
                <button id="free-button" class="free-button" onclick="setStatus('free')">Free</button>
            </div>
        </div>
    </div>

    <script>
        // Load the current status when the page loads
        window.onload = function() {
            fetchCurrentStatus();
        };
        
        // Fetch the current status from the server
        function fetchCurrentStatus() {
            fetch('/status')
                .then(response => response.text())
                .then(status => {
                    document.getElementById('current-status').textContent = 
                        status === 'dnd' ? 'Do Not Disturb' : 'Free';
                })
                .catch(error => {
                    console.error('Error fetching status:', error);
                });
        }
        
        // Set a new status
        function setStatus(status) {
            fetch('/status', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ status: status }),
            })
            .then(response => response.text())
            .then(result => {
                document.getElementById('current-status').textContent = 
                    status === 'dnd' ? 'Do Not Disturb' : 'Free';
            })
            .catch(error => {
                console.error('Error setting status:', error);
            });
        }
    </script>
</body>
</html>"#;

// Need lots of stack to parse JSON
const STACK_SIZE: usize = 10240;
// Max payload length
const MAX_LEN: usize = 128;

// Shared state between threads
static REQUEST_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static DND_MODE: AtomicBool = AtomicBool::new(false); // false = "Free", true = "Do Not Disturb"

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    // Get peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Initialize the SSD1306 OLED display
    // Note: Adjust the pins according to your wiring
    let i2c = i2c::I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio21, // SDA
        peripherals.pins.gpio22, // SCL
        &i2c::I2cConfig::new().baudrate(400.kHz().into()),
    )?;

    // OLED Display address is typically 0x3C or 0x3D
    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    // Initialize display
    display.init().unwrap();
    display.clear(BinaryColor::On).unwrap();

    // Setup WiFi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    // Display connecting message
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("Connecting to WiFi...", Point::new(0, 10), text_style)
        .draw(&mut display)
        .unwrap();
    display.flush().unwrap();

    // Connect to WiFi network
    connect_wifi(&mut wifi)?;

    // Get and display IP address
    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    info!("Wifi DHCP info: {:?}", ip_info);
    info!("HTTP server will be available at http://{}/", ip_info.ip);

    // Update display with initial status
    update_display(&mut display, text_style, &ip_info, "Free", 0)?;

    // Create HTTP server
    let server_config = HttpConfiguration {
        stack_size: STACK_SIZE,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_config)?;

    // Set up routes
    // Route for serving the main HTML page
    server.fn_handler::<anyhow::Error, _>("/", Method::Get, |req| {
        // Increment request counter
        REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

        let mut resp = req.into_ok_response()?;
        resp.write_all(INDEX_HTML.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    // Route for handling POST requests with JSON
    server.fn_handler::<anyhow::Error, _>("/post", Method::Post, |mut req| {
        use embedded_svc::io::Read;
        use serde::Deserialize;

        // Increment request counter
        REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

        #[derive(Deserialize)]
        struct FormData<'a> {
            first_name: &'a str,
            age: u32,
            birthplace: &'a str,
        }

        let len = req.content_len().unwrap_or(0) as usize;

        if len > MAX_LEN {
            req.into_status_response(413)?
                .write_all("Request too big".as_bytes())?;
            return Ok(());
        }

        let mut buf = vec![0; len];
        req.read_exact(&mut buf)?;
        let mut resp = req.into_ok_response()?;

        if let Ok(form) = serde_json::from_slice::<FormData>(&buf) {
            write!(
                resp,
                "Hello, {}-year-old {} from {}!",
                form.age, form.first_name, form.birthplace
            )?;
        } else {
            resp.write_all("JSON error".as_bytes())?;
        }

        Ok(())
    })?;

    // Route for getting current status
    server.fn_handler::<anyhow::Error, _>("/status", Method::Get, |req| {
        let mut resp = req.into_ok_response()?;

        let is_dnd = DND_MODE.load(Ordering::SeqCst);
        let status = if is_dnd { "dnd" } else { "free" };

        resp.write_all(status.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    // Route for setting status
    server.fn_handler::<anyhow::Error, _>("/status", Method::Post, |mut req| {
        use embedded_svc::io::Read;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct StatusData<'a> {
            status: &'a str,
        }

        let len = req.content_len().unwrap_or(0) as usize;

        if len > MAX_LEN {
            req.into_status_response(413)?
                .write_all("Request too big".as_bytes())?;
            return Ok(());
        }

        let mut buf = vec![0; len];
        req.read_exact(&mut buf)?;
        let mut resp = req.into_ok_response()?;

        if let Ok(data) = serde_json::from_slice::<StatusData>(&buf) {
            match data.status {
                "dnd" => {
                    DND_MODE.store(true, Ordering::SeqCst);
                    resp.write_all("Status set to Do Not Disturb".as_bytes())?;
                }
                "free" => {
                    DND_MODE.store(false, Ordering::SeqCst);
                    resp.write_all("Status set to Free".as_bytes())?;
                }
                _ => {
                    resp.write_all("Invalid status".as_bytes())?;
                }
            }
        } else {
            resp.write_all("JSON error".as_bytes())?;
        }

        Ok(())
    })?;

    info!("HTTP server started and running");

    // Keep the application running and update display periodically
    let mut last_counter = 0;
    let mut last_dnd = false;

    loop {
        // Get current values
        let current_counter = REQUEST_COUNTER.load(Ordering::SeqCst);
        let current_dnd = DND_MODE.load(Ordering::SeqCst);

        // Update display if either counter or DND status has changed
        if current_counter != last_counter || current_dnd != last_dnd {
            let status_text = if current_dnd {
                "Do Not Disturb"
            } else {
                "Free"
            };

            // Update the display with current status
            update_display(&mut display, text_style, &ip_info, status_text, current_counter)?;

            last_counter = current_counter;
            last_dnd = current_dnd;
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // This line will never be reached
    #[allow(unreachable_code)]
    Ok(())
}

// Helper function to update the display
fn update_display(
    display: &mut Ssd1306<I2CInterface<i2c::I2cDriver<'_>>, DisplaySize128x32, BufferedGraphicsMode<DisplaySize128x32>>,
    text_style: MonoTextStyle<BinaryColor>,
    ip_info: &embedded_svc::ipv4::IpInfo,
    status: &str,
    requests: u32,
) -> anyhow::Result<()> {
    display.clear(BinaryColor::Off).unwrap();

    Text::new("WiFi Connected", Point::new(0, 10), text_style)
        .draw(display)
        .unwrap();

    Text::new(
        &format!("IP: {}", ip_info.ip),
        Point::new(0, 25),
        text_style,
    )
    .draw(display)
    .unwrap();

    Text::new(
        &format!("Status: {}", status),
        Point::new(0, 40),
        text_style,
    )
    .draw(display)
    .unwrap();

    Text::new(
        &format!("Requests: {}", requests),
        Point::new(0, 55),
        text_style,
    )
    .draw(display)
    .unwrap();

    display.flush().unwrap();
    
    Ok(())
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.try_into().unwrap(),
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;
    wifi.start()?;
    info!("Wifi started");
    wifi.connect()?;
    info!("Wifi connected");
    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(())
}
