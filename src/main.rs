#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{collections::HashMap, fs};

use anyhow::Context;
use serde::Deserialize;

const HID_USAGE: u16 = 0x61;
const HID_USAGE_PAGE: u16 = 0xFF60;

#[derive(Debug, Deserialize)]
struct Config {
    keyboards: HashMap<String, KeyboardConfig>,
}

#[derive(Debug, Deserialize)]
struct KeyboardConfig {
    vendor_id: u16,
    product_id: u16,
}

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
    let mut config_path = dirs::config_local_dir().context("Could not find config path")?;
    config_path.push("keeb_os_probe.toml");
    let config_toml =
        fs::read_to_string(&config_path).context(format!("Config path: {:?}", &config_path))?;
    let config: Config = toml::from_str(&config_toml)?;
    let board_connection = BoardConnection::new(&config)?;
    board_connection.probe()
}

struct BoardConnection<'a> {
    hid_api: hidapi::HidApi,
    config: &'a Config,
}
impl<'a> BoardConnection<'a> {
    pub fn new(config: &'a Config) -> anyhow::Result<Self> {
        Ok(Self {
            hid_api: hidapi::HidApi::new()?,
            config,
        })
    }

    pub fn probe(&self) -> anyhow::Result<()> {
        for (keeb, keeb_config) in &self.config.keyboards {
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
