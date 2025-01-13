#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{collections::HashMap, fs, thread, time::Duration};

use anyhow::Context;
use rusb::UsbContext;
use serde::Deserialize;

const HID_USAGE: u16 = 0x61;
const HID_USAGE_PAGE: u16 = 0xFF60;

/// [QMK OS enum](https://github.com/qmk/qmk_firmware/blob/26f898c8a538b808cf506f558a9454f7f50e3ba6/quantum/os_detection.h#L23)
#[cfg(target_os = "linux")]
const HOST_OS_CODE: u8 = 1;
#[cfg(target_os = "windows")]
const HOST_OS_CODE: u8 = 2;
#[cfg(target_os = "macos")]
const HOST_OS_CODE: u8 = 3;

/// Try to connect to the configured HID device(s)
/// and send HID messages passing the current host OS code
pub fn main() -> anyhow::Result<()> {
    if !rusb::has_hotplug() {
        anyhow::bail!("No hotplug compat");
    }
    let mut config_path = dirs::config_local_dir().context("Could not find config path")?;
    config_path.push("keeb_os_probe.toml");
    let config_toml =
        fs::read_to_string(&config_path).context(format!("Config path: {:?}", &config_path))?;
    let config: Config = toml::from_str(&config_toml)?;
    let board_connection = BoardConnection::new(config)?;
    let context = rusb::Context::new()?;
    let _reg = rusb::HotplugBuilder::new()
        .enumerate(true)
        .register::<rusb::Context, _>(&context, Box::new(board_connection))?;
    loop {
        context.handle_events(None)?;
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    keyboards: HashMap<String, KeyboardConfig>,
}

#[derive(Debug, Deserialize)]
struct KeyboardConfig {
    vendor_id: u16,
    product_id: u16,
}

struct BoardConnection {
    hid_api: hidapi::HidApi,
    config: Config,
}
impl BoardConnection {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        Ok(Self {
            hid_api: hidapi::HidApi::new()?,
            config,
        })
    }

    pub fn probe(&self, vendor_id: u16, product_id: u16) -> anyhow::Result<()> {
        if let Some((keeb, keeb_config)) = &self.config.keyboards.iter().find(|(_, keeb_config)| {
            keeb_config.vendor_id == vendor_id && keeb_config.product_id == product_id
        }) {
            thread::sleep(Duration::from_millis(50));
            let Some(device) = self.hid_api.device_list().find(|device| {
                device.vendor_id() == keeb_config.vendor_id
                    && device.product_id() == keeb_config.product_id
                    && device.usage() == HID_USAGE
                    && device.usage_page() == HID_USAGE_PAGE
            }) else {
                eprintln!("Keeb '{keeb}' not connected");
                return Ok(());
            };
            let device = self.hid_api.open_path(device.path())?;
            device.write(&[
                0, // report ID - mandatory
                // the actual payload starts here, limited to 32 bytes in QMK (or by HID in general?)
                42, // reporting host
                HOST_OS_CODE,
            ])?;
        }
        Ok(())
    }
}
impl<T: rusb::UsbContext> rusb::Hotplug<T> for BoardConnection {
    fn device_arrived(&mut self, device: rusb::Device<T>) {
        if let Ok(desc) = device.device_descriptor() {
            self.probe(desc.vendor_id(), desc.product_id())
                .expect("Probed device");
        }
    }

    fn device_left(&mut self, _device: rusb::Device<T>) {}
}
