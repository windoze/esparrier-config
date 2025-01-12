use std::{net::Ipv4Addr, str::FromStr};

use log::debug;
use nusb::{
    transfer::{Direction, RequestBuffer},
    DeviceInfo,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Device not found")]
    DeviceNotFound,

    #[error("Unknown device")]
    UnknownDevice,

    #[error("Device busy")]
    DeviceBusy,

    #[error("Permission denied")]
    PermissionDenied,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    ActiveConfiguration(#[from] nusb::descriptors::ActiveConfigurationError),

    #[error("Transfer failed {0}")]
    TransferFailed(#[from] nusb::transfer::TransferError),

    #[error("Invalid response")]
    InvalidResponse,

    #[error("Format error, {0}")]
    FormatError(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EsparrierState {
    pub version_major: u8,
    pub version_minor: u8,
    pub version_patch: u8,
    pub feature_flags: u8,
    pub ip_address: Ipv4Addr,
    pub ip_prefix: u8,
    pub server_connected: bool,
    pub active: bool,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Hash, Eq, PartialEq)]
pub enum FeatureFlag {
    Led = 0b_0001_0001,
    SmartLed = 0b_0001_0010,
    Graphics = 0b_0001_0100,
    Clipboard = 0b_0001_0000,
}

impl EsparrierState {
    fn from_bytes(bytes: &[u8]) -> Self {
        EsparrierState {
            version_major: bytes[1],
            version_minor: bytes[2],
            version_patch: bytes[3],
            feature_flags: bytes[4],
            ip_address: Ipv4Addr::new(bytes[5], bytes[6], bytes[7], bytes[8]),
            ip_prefix: bytes[9],
            server_connected: bytes[10] != 0,
            active: bytes[11] != 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct EsparrierConfig {
    // These fields must be set
    pub ssid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password: String,
    pub server: String,
    pub screen_name: String,

    // Screen configuration
    #[serde(default = "get_default_screen_width")]
    pub screen_width: u16,
    #[serde(default = "get_default_screen_height")]
    pub screen_height: u16,
    #[serde(default)]
    pub flip_wheel: bool,

    // LED configuration
    #[serde(default = "get_default_brightness")]
    pub brightness: u8,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ip_addr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gateway: Option<String>,

    // USB HID configuration
    #[serde(default = "get_default_vid", skip_serializing_if = "is_default_vid")]
    pub vid: u16,
    #[serde(default = "get_default_pid", skip_serializing_if = "is_default_pid")]
    pub pid: u16,
    #[serde(
        default = "get_default_manufacturer",
        skip_serializing_if = "is_default_manufacturer"
    )]
    pub manufacturer: String,
    #[serde(
        default = "get_default_product",
        skip_serializing_if = "is_default_product"
    )]
    pub product: String,
    #[serde(
        default = "get_default_serial_number",
        skip_serializing_if = "is_default_serial_number"
    )]
    pub serial_number: String,

    // Misc internal fields
    #[serde(
        default = "get_default_watchdog_timeout",
        skip_serializing_if = "is_default_watchdog_timeout"
    )]
    pub watchdog_timeout: u32,
}

pub const SCREEN_WIDTH: u16 = 1920;
pub const SCREEN_HEIGHT: u16 = 1080;
pub const REVERSED_WHEEL: bool = false;
pub const BRIGHTNESS: u8 = 30;
pub const USB_VID: u16 = 0x0d0a;
pub const USB_PID: u16 = 0xc0de;
pub const USB_MANUFACTURER: &str = "0d0a.com";
pub const USB_PRODUCT: &str = "Esparrier KVM";
pub const USB_SERIAL_NUMBER: &str = "88888888";
pub const WATCHDOG_TIMEOUT: u32 = 15;

// Kinda stupid
fn get_default_screen_width() -> u16 {
    SCREEN_WIDTH
}

fn get_default_screen_height() -> u16 {
    SCREEN_HEIGHT
}

fn get_default_brightness() -> u8 {
    BRIGHTNESS
}

fn get_default_vid() -> u16 {
    USB_VID
}

fn is_default_vid(vid: &u16) -> bool {
    *vid == USB_VID
}

fn get_default_pid() -> u16 {
    USB_PID
}

fn is_default_pid(pid: &u16) -> bool {
    *pid == USB_PID
}

fn get_default_manufacturer() -> String {
    String::from_str(USB_MANUFACTURER).unwrap()
}

fn is_default_manufacturer(manufacturer: &String) -> bool {
    *manufacturer == USB_MANUFACTURER
}

fn get_default_product() -> String {
    String::from_str(USB_PRODUCT).unwrap()
}

fn is_default_product(product: &String) -> bool {
    *product == USB_PRODUCT
}

fn get_default_serial_number() -> String {
    String::from_str(USB_SERIAL_NUMBER).unwrap()
}

fn is_default_serial_number(serial_number: &String) -> bool {
    *serial_number == USB_SERIAL_NUMBER
}

fn get_default_watchdog_timeout() -> u32 {
    WATCHDOG_TIMEOUT
}

fn is_default_watchdog_timeout(timeout: &u32) -> bool {
    *timeout == WATCHDOG_TIMEOUT
}

pub struct Esparrier {
    pub device_info: DeviceInfo,

    interface: nusb::Interface,
    ep_in: u8,
    ep_out: u8,
}

impl Esparrier {
    pub fn auto_detect<A, B, C, D>(vid: A, pid: B, bus: C, address: D) -> Option<Self>
    where
        A: Into<Option<u16>> + Clone,
        B: Into<Option<u16>> + Clone,
        C: Into<Option<u8>> + Clone,
        D: Into<Option<u8>> + Clone,
    {
        nusb::list_devices().ok().and_then(|l| {
            l.filter(|di| {
                vid.clone().into().map_or(true, |v| di.vendor_id() == v)
                    && pid.clone().into().map_or(true, |p| di.product_id() == p)
                    && bus.clone().into().map_or(true, |p| di.bus_number() == p)
                    && address
                        .clone()
                        .into()
                        .map_or(true, |p| di.device_address() == p)
            })
            .find_map(|di| match Self::try_open_device(di) {
                Ok(d) => Some(d),
                Err(_) => None,
            })
        })
    }

    pub async fn get_state(&self) -> Result<EsparrierState, Error> {
        // Send the 's'(GetState) command to the device
        self.write(b"s").await?;
        let result = self.read().await?;
        if result.len() != 12 || result[0] != b's' {
            return Err(Error::InvalidResponse);
        }
        Ok(EsparrierState::from_bytes(&result))
    }

    pub async fn get_config(&self) -> Result<EsparrierConfig, Error> {
        // Send the 'r'(ReadConfig) command to the device
        self.write(b"r").await?;

        // Response format: ['r', <num_blocks>], <block1>, <block2>, ...
        let result = self.read().await?;
        if result.len() != 2 || result[0] != b'r' {
            return Err(Error::InvalidResponse);
        }
        let size = result[1] as usize;
        debug!("Blocks: {}", size);
        let mut data = Vec::new();
        for _ in 0..size {
            let result = self.read().await?;
            debug!("Block len: {}", result.len());
            let s = result.strip_suffix(&[0]).unwrap_or(&result);
            data.extend_from_slice(s);
        }
        data.retain(|&b| (b != 0) && (b <= 0xF4));
        let config: EsparrierConfig = serde_json::from_slice(&data)
            .map_err(|_| Error::FormatError("Invalid JSON format".to_string()))?;
        Ok(config)
    }

    pub async fn set_config(&self, config: EsparrierConfig) -> Result<(), Error> {
        if config.ssid.is_empty()
            || config.password.is_empty()
            || config.server.is_empty()
            || config.screen_name.is_empty()
        {
            return Err(Error::FormatError(
                "Invalid config, required fields are empty".to_string(),
            ));
        }
        let data = serde_json::to_vec(&config)
            .map_err(|_| Error::FormatError("Invalid JSON format".to_string()))?;
        let blocks = data.chunks(64).collect::<Vec<_>>();
        // Send the 'w'(WriteConfig) command to the device
        self.write(&[b'w', blocks.len() as u8]).await?;
        // Send the blocks
        for block in blocks {
            self.write(block).await?;
        }
        // Receive the 'o'(Ok) response
        let result = self.read().await?;
        if result.len() != 1 || result[0] != b'o' {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }

    // Commit will flash the new config and restart the device.
    // The current connection will be lost, so this method consumes the instance.
    // The caller should wait for few seconds before trying to connect again,
    // or setup a watcher to detect when the device is back online.
    pub async fn commit_config(self) -> Result<(), Error> {
        // Send the 'c'(CommitConfig) command to the device
        self.write(b"c").await?;
        // Receive the 'o'(Ok) response
        let result = self.read().await?;
        if result.len() != 1 || result[0] != b'o' {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }

    fn try_open_device(di: DeviceInfo) -> Result<Self, Error> {
        let device = di.open()?;
        let cfg = device.active_configuration()?;

        // Find the interface with class 0xFF, subclass 0x0D, and protocol 0x0A
        let iface_alt = cfg
            .interface_alt_settings()
            .find(|i| i.class() == 0xFF && i.subclass() == 0x0D && i.protocol() == 0x0A)
            .ok_or(Error::UnknownDevice)?;

        // Claim this interface
        let interface = device.claim_interface(iface_alt.interface_number())?;

        // Find the bulk IN and OUT endpoints
        let alt = interface.descriptors().next().ok_or(Error::UnknownDevice)?;
        let ep_in = alt
            .endpoints()
            .find(|ep| ep.direction() == Direction::In)
            .ok_or(Error::UnknownDevice)?
            .address();
        let ep_out = alt
            .endpoints()
            .find(|ep| ep.direction() == Direction::Out)
            .ok_or(Error::UnknownDevice)?
            .address();

        Ok(Self {
            device_info: di,
            interface,
            ep_in,
            ep_out,
        })
    }

    async fn write(&self, buffer: &[u8]) -> Result<(), Error> {
        self.interface
            .bulk_out(self.ep_out, buffer.into())
            .await
            .into_result()?;
        Ok(())
    }

    async fn read(&self) -> Result<Vec<u8>, Error> {
        let ret = self
            .interface
            .bulk_in(self.ep_in, RequestBuffer::new(64))
            .await
            .into_result()?;
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_config() {
        let config_str = r#"{
            "ssid": "some-wifi",
            "password": "magic-word",
            "server": "192.168.2.59:24800",
            "screen_name": "SAW",
            "screen_width": 5120,
            "screen_height": 2880,
            "flip_wheel": true,
            "brightness": 10,
            "serial_number": "88888888",
            "pid": 4
        }"#;
        let config: EsparrierConfig = serde_json::from_str(config_str).unwrap();
        println!("{:?}", config);
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_get_state() {
        let esparrier = Esparrier::auto_detect(None, None, None, None).unwrap();
        let state = esparrier.get_state().await;
        println!("{:?}", state);
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_get_config() {
        let esparrier = Esparrier::auto_detect(None, None, None, None).unwrap();
        let config = esparrier.get_config().await.unwrap();
        println!("{:?}", config);
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_set_config() {
        let esparrier = Esparrier::auto_detect(None, None, None, None).unwrap();
        let config = serde_json::from_str(
            r#"{
            "ssid": "some-wifi",
            "password": "magic-word",
            "server": "192.168.2.59:24800",
            "screen_name": "SAW",
            "screen_width": 5120,
            "screen_height": 2880,
            "flip_wheel": true,
            "brightness": 10,
            "serial_number": "88888888",
            "pid": 4
        }"#,
        )
        .unwrap();
        println!("{}", serde_json::to_string_pretty(&config).unwrap());
        esparrier.set_config(config).await.unwrap();
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_set_config_1() {
        let esparrier = Esparrier::auto_detect(None, None, None, None).unwrap();
        let mut config = esparrier.get_config().await.unwrap();
        config.ssid = "test".to_string();
        esparrier.set_config(config).await.unwrap();
    }

    #[ignore = "This will reset the device"]
    #[tokio::test]
    async fn test_commit_config() {
        let esparrier = Esparrier::auto_detect(None, None, None, None).unwrap();
        let mut config = esparrier.get_config().await.unwrap();
        config.ssid = "test".to_string();
        esparrier.set_config(config).await.unwrap();
        esparrier.commit_config().await.unwrap();
    }
}
