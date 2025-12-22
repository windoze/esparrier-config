use std::{
    net::{Ipv4Addr, SocketAddrV4},
    str::FromStr,
};

use futures::StreamExt;
use log::debug;
use nusb::{
    hotplug::HotplugEvent,
    transfer::{Bulk, Buffer, Direction, In, Out},
    DeviceInfo, Endpoint, ErrorKind,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Config field '{0}' is empty")]
    FieldEmpty(String),

    #[error("Config field '{0}' is too long")]
    FieldTooLong(String),

    #[error("Config field '{0}' is out of range [{1}..{2}]")]
    FieldOutOfRange(String, usize, usize),

    #[error("Config field '{0}' is invalid endpoint")]
    InvalidEndpoint(String),

    #[error("Config field '{0}' is invalid IP address")]
    InvalidIpAddress(String),

    #[error("Config field '{0}' has invalid IPv4 CIDR prefix")]
    InvalidIpCidrPrefix(String),
}

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
    ActiveConfiguration(#[from] nusb::ActiveConfigurationError),

    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),

    #[error("Transfer failed {0}")]
    TransferFailed(#[from] nusb::transfer::TransferError),

    #[error("Invalid response")]
    InvalidResponse,

    #[error("Format error, {0}")]
    FormatError(String),

    #[error(transparent)]
    ConfigError(#[from] ConfigError),

    #[error("OTA not supported by this firmware")]
    OtaNotSupported,

    #[error("OTA error: {0}")]
    OtaError(String),
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
    pub keep_awake: bool,
    pub model_id: u8,
}

/// Feature flags indicating device capabilities.
/// These match the firmware's FEATURE_FLAGS in constants.rs.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Hash, Eq, PartialEq)]
pub enum FeatureFlag {
    Led = 0b_0000_0001,
    SmartLed = 0b_0000_0010,
    Graphics = 0b_0000_0100,
    Ota = 0b_0100_0000,
    Clipboard = 0b_1000_0000,
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
            keep_awake: bytes[12] != 0,
            model_id: bytes[13],
        }
    }

    /// Check if a specific feature flag is set.
    pub fn has_feature(&self, flag: FeatureFlag) -> bool {
        self.feature_flags & (flag as u8) != 0
    }

    /// Check if OTA updates are supported by the firmware.
    pub fn has_ota_support(&self) -> bool {
        self.has_feature(FeatureFlag::Ota)
    }

    /// Get the firmware version as a tuple (major, minor, patch).
    pub fn version(&self) -> (u8, u8, u8) {
        (self.version_major, self.version_minor, self.version_patch)
    }

    /// Get the firmware version as a string (e.g., "0.7.0").
    pub fn version_string(&self) -> String {
        format!(
            "{}.{}.{}",
            self.version_major, self.version_minor, self.version_patch
        )
    }

    /// Get the model name for this device based on model_id.
    /// Returns None if the model_id is unknown.
    pub fn model_name(&self) -> Option<&'static str> {
        model_id_to_name(self.model_id)
    }
}

/// Map model_id to firmware asset name prefix.
/// These correspond to the asset names in GitHub releases.
pub fn model_id_to_name(model_id: u8) -> Option<&'static str> {
    match model_id {
        1 => Some("m5atoms3-lite"),
        2 => Some("m5atoms3"),
        3 => Some("m5atoms3r"),
        4 => Some("devkitc-1_0"),
        5 => Some("devkitc-1_1"),
        6 => Some("xiao-esp32s3"),
        7 => Some("esp32-s3-eth"),
        255 => Some("generic"),
        _ => None,
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
    #[serde(
        skip_serializing_if = "is_default_polling_rate",
        default = "get_default_polling_rate"
    )]
    pub polling_rate: u16,
    #[serde(
        skip_serializing_if = "is_default_jiggle_interval",
        default = "get_default_jiggle_interval"
    )]
    pub jiggle_interval: u16,

    // LED configuration
    #[serde(default = "get_default_brightness")]
    pub brightness: u8,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ip_addr: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dns_server: Vec<String>,
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
    #[serde(
        default = "get_default_landing_url",
        skip_serializing_if = "is_default_landing_url"
    )]
    pub landing_url: String,

    // Misc internal fields
    #[serde(
        default = "get_default_watchdog_timeout",
        skip_serializing_if = "is_default_watchdog_timeout"
    )]
    pub watchdog_timeout: u32,
}

impl EsparrierConfig {
    pub fn validate(&self) -> Result<(), Error> {
        fn validate_string(s: &str, name: &str, max_len: usize) -> Result<(), Error> {
            if s.is_empty() {
                Err(ConfigError::FieldEmpty(name.to_string()).into())
            } else if s.len() > max_len {
                Err(ConfigError::FieldTooLong(name.to_string()).into())
            } else {
                Ok(())
            }
        }

        macro_rules! validate_string {
            ($s:ident, $max_len:literal) => {
                validate_string(&self.$s, stringify!($s), $max_len)?;
            };
            () => {};
        }

        macro_rules! validate_num {
            ($s:ident, $min:literal, $max:literal) => {
                if self.$s < $min || self.$s > $max {
                    return Err(ConfigError::FieldOutOfRange(
                        stringify!($s).to_string(),
                        $min,
                        $max,
                    )
                    .into());
                }
            };
            () => {};
        }

        validate_string!(ssid, 32);
        validate_string!(password, 64);
        validate_string!(server, 64);
        if self.server.parse::<SocketAddrV4>().is_err() {
            return Err(ConfigError::InvalidEndpoint("server".to_string()).into());
        }
        validate_string!(screen_name, 64);
        validate_num!(screen_width, 1, 32767);
        validate_num!(screen_height, 1, 32767);
        validate_num!(brightness, 1, 100);

        if let Some(ip) = &self.ip_addr {
            let (ip, prefix) =
                ip.split_once('/')
                    .ok_or(Into::<Error>::into(ConfigError::InvalidIpAddress(
                        "ip_addr".to_string(),
                    )))?;
            let _ip = Ipv4Addr::from_str(ip).map_err(|_| {
                Into::<Error>::into(ConfigError::InvalidIpAddress("ip_addr".to_string()))
            })?;
            if prefix.parse::<u8>().is_err() {
                return Err(ConfigError::InvalidIpCidrPrefix("ip_addr".to_string()).into());
            }
        }
        for d in &self.dns_server {
            let _ip = Ipv4Addr::from_str(d).map_err(|_| {
                Into::<Error>::into(ConfigError::InvalidIpAddress("dns_server".to_string()))
            })?;
        }
        if let Some(gateway) = &self.gateway {
            let _ip = Ipv4Addr::from_str(gateway).map_err(|_| {
                Into::<Error>::into(ConfigError::InvalidIpAddress("gateway".to_string()))
            })?;
        }
        validate_string!(manufacturer, 64);
        validate_string!(product, 64);
        validate_string!(serial_number, 64);
        // The landing URL can be empty
        if self.landing_url.len() > 255 {
            return Err(ConfigError::FieldTooLong("landing_url".to_string()).into());
        }
        Ok(())
    }
}

pub const SCREEN_WIDTH: u16 = 1920;
pub const SCREEN_HEIGHT: u16 = 1080;
pub const REVERSED_WHEEL: bool = false;
pub const BRIGHTNESS: u8 = 30;
pub const POLLING_RATE: u16 = 200;
pub const JIGGLE_INTERVAL: u16 = 60;
pub const USB_VID: u16 = 0x0d0a;
pub const USB_PID: u16 = 0xc0de;
pub const USB_MANUFACTURER: &str = "0d0a.com";
pub const LANDING_URL: &str = "https://0d0a.com";
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

fn get_default_polling_rate() -> u16 {
    POLLING_RATE
}

fn is_default_polling_rate(polling_rate: &u16) -> bool {
    *polling_rate == POLLING_RATE
}

fn get_default_jiggle_interval() -> u16 {
    JIGGLE_INTERVAL
}

fn is_default_jiggle_interval(jiggle_interval: &u16) -> bool {
    *jiggle_interval == JIGGLE_INTERVAL
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

fn get_default_landing_url() -> String {
    String::from_str(LANDING_URL).unwrap()
}

fn is_default_landing_url(serial_number: &String) -> bool {
    *serial_number == LANDING_URL
}

fn get_default_watchdog_timeout() -> u32 {
    WATCHDOG_TIMEOUT
}

fn is_default_watchdog_timeout(timeout: &u32) -> bool {
    *timeout == WATCHDOG_TIMEOUT
}

pub struct Esparrier {
    pub device_info: DeviceInfo,

    ep_in: Mutex<Endpoint<Bulk, In>>,
    ep_out: Mutex<Endpoint<Bulk, Out>>,
}

/// Compare bus IDs, normalizing numeric values (e.g., "3" matches "03")
fn bus_id_matches(device_bus_id: &str, filter_bus_id: &str) -> bool {
    // First try exact match
    if device_bus_id == filter_bus_id {
        return true;
    }
    // Try numeric comparison (handles "3" == "03" case)
    if let (Ok(a), Ok(b)) = (
        device_bus_id.parse::<u32>(),
        filter_bus_id.parse::<u32>(),
    ) {
        return a == b;
    }
    false
}

impl Esparrier {
    pub async fn list_devices(vid: Option<u16>, pid: Option<u16>) -> Vec<(String, u8)> {
        let devices = match nusb::list_devices().await {
            Ok(d) => d,
            Err(e) => {
                debug!("Failed to list devices: {e}");
                return Vec::new();
            }
        };
        let mut ret = Vec::new();
        for di in devices {
            if di.vendor_id() == vid.unwrap_or(USB_VID) && di.product_id() == pid.unwrap_or(USB_PID)
            {
                ret.push((di.bus_id().to_string(), di.device_address()));
            }
        }
        ret
    }

    /**
     * Auto detect the device with the specified VID, PID, bus ID, and device address.
     * If `wait` is true, the method will wait for the device to be connected.
     */
    pub async fn auto_detect<A, B, C, D>(
        wait: bool,
        vid: A,
        pid: B,
        bus: C,
        address: D,
    ) -> Option<Self>
    where
        A: Into<Option<u16>> + Clone,
        B: Into<Option<u16>> + Clone,
        C: Into<Option<String>> + Clone,
        D: Into<Option<u8>> + Clone,
    {
        if wait {
            return Self::wait_for_device(vid, pid, bus, address).await.ok();
        }
        let devices = match nusb::list_devices().await {
            Ok(d) => d,
            Err(_) => return None,
        };
        for di in devices {
            if vid.clone().into().is_none_or(|v| di.vendor_id() == v)
                && pid.clone().into().is_none_or(|p| di.product_id() == p)
                && bus
                    .clone()
                    .into()
                    .is_none_or(|b| bus_id_matches(di.bus_id(), &b))
                && address
                    .clone()
                    .into()
                    .is_none_or(|a| di.device_address() == a)
            {
                if let Ok(dev) = Self::try_open_device(di).await {
                    return Some(dev);
                }
            }
        }
        None
    }

    /// Get the current state from the device.
    pub async fn get_state(&self) -> Result<EsparrierState, Error> {
        // Send the 's'(GetState) command to the device
        self.write(b"s").await?;
        let result = self.read().await?;
        if result.len() < 13 || result[0] != b's' {
            return Err(Error::InvalidResponse);
        }
        Ok(EsparrierState::from_bytes(&result))
    }

    /// Get the current configuration from the device.
    pub async fn get_config(&self) -> Result<EsparrierConfig, Error> {
        // Send the 'r'(ReadConfig) command to the device
        self.write(b"r").await?;

        // Response format: ['r', <num_blocks>], <block1>, <block2>, ...
        let result = self.read().await?;
        if result.len() != 2 || result[0] != b'r' {
            return Err(Error::InvalidResponse);
        }
        let size = result[1] as usize;
        debug!("Blocks: {size}");
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

    /// Upload the new configuration to the device.
    pub async fn set_config(&self, config: EsparrierConfig) -> Result<(), Error> {
        config.validate()?;
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

    /// Commit will flash the new config and restart the device.
    /// The current connection will be lost, so this method consumes the instance.
    /// The caller should wait for few seconds before trying to connect again,
    /// or setup a watcher to detect when the device is back online.
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

    /// Reboot the device.
    /// The current connection will be lost, so this method consumes the instance.
    /// The caller should wait for few seconds before trying to connect again,
    /// or setup a watcher to detect when the device is back online.
    pub async fn reboot_device(self) -> Result<(), Error> {
        // Send the 'b'(Reboot) command to the device
        self.write(b"b").await?;
        // Receive the 'o'(Ok) response
        let result = self.read().await?;
        if result.len() != 1 || result[0] != b'o' {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }

    pub async fn keep_awake(&self, enable: bool) -> Result<(), Error> {
        // Send the 'k'(KeepAwake) command to the device
        self.write(&[b'k', enable as u8]).await?;
        // Receive the 'o'(Ok) response
        let result = self.read().await?;
        if result.len() != 1 || result[0] != b'o' {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }

    /// Upload firmware via OTA.
    ///
    /// This method uploads the firmware binary to the device in chunks.
    /// The device will verify the CRC32 checksum and reboot automatically on success.
    ///
    /// # Arguments
    /// * `firmware` - The firmware binary data
    /// * `progress_callback` - Optional callback for progress updates (received_bytes, total_bytes)
    ///
    /// # Returns
    /// * `Ok(())` - OTA completed successfully, device will reboot
    /// * `Err(Error)` - OTA failed
    ///
    /// # Protocol
    /// 1. Send OtaStart command: 'O' + size(4B LE) + crc32(4B LE)
    /// 2. Send OtaData chunks: 'D' + packets(1B) + length(2B LE) followed by packets × 64 bytes
    /// 3. Receive OtaProgress or OtaComplete responses
    pub async fn upload_ota<F>(
        &self,
        firmware: &[u8],
        mut progress_callback: Option<F>,
    ) -> Result<(), Error>
    where
        F: FnMut(usize, usize),
    {
        let total_size = firmware.len();
        if total_size == 0 || total_size > 0x100000 {
            return Err(Error::OtaError(format!(
                "Invalid firmware size: {} (max 1048576 bytes)",
                total_size
            )));
        }

        // Calculate CRC32 (IEEE 802.3 polynomial, same as firmware)
        let crc = crc32(firmware);
        debug!("Firmware size: {}, CRC32: 0x{:08x}", total_size, crc);

        // Send OtaStart command: 'O' + size(4B LE) + crc(4B LE)
        let mut start_cmd = [0u8; 9];
        start_cmd[0] = b'O';
        start_cmd[1..5].copy_from_slice(&(total_size as u32).to_le_bytes());
        start_cmd[5..9].copy_from_slice(&crc.to_le_bytes());
        self.write(&start_cmd).await?;

        // Receive response
        let result = self.read().await?;
        if result.is_empty() {
            return Err(Error::InvalidResponse);
        }
        if result[0] == b'e' {
            return Err(self.parse_ota_error(&result));
        }
        if result[0] != b'o' {
            return Err(Error::InvalidResponse);
        }

        // Send firmware in chunks (up to 4096 bytes per chunk = 64 packets × 64 bytes)
        const CHUNK_SIZE: usize = 4096;
        let mut sent = 0usize;

        for chunk in firmware.chunks(CHUNK_SIZE) {
            let chunk_len = chunk.len();
            // Calculate number of 64-byte USB packets needed (round up)
            let packets = chunk_len.div_ceil(64) as u8;

            // Send OtaData command: 'D' + packets(1B) + length(2B LE)
            let length_bytes = (chunk_len as u16).to_le_bytes();
            self.write(&[b'D', packets, length_bytes[0], length_bytes[1]])
                .await?;

            // Send the data packets
            for packet_data in chunk.chunks(64) {
                // Pad to 64 bytes if needed (USB bulk transfer)
                let mut packet = [0u8; 64];
                packet[..packet_data.len()].copy_from_slice(packet_data);
                self.write(&packet).await?;
            }

            sent += chunk_len;

            // Call progress callback
            if let Some(ref mut cb) = progress_callback {
                cb(sent, total_size);
            }

            // Receive response (Progress or Complete or Error)
            let result = self.read().await?;
            if result.is_empty() {
                return Err(Error::InvalidResponse);
            }

            match result[0] {
                b'P' => {
                    // Progress response: 'P' + received(4B LE) + total(4B LE)
                    if result.len() >= 9 {
                        let received =
                            u32::from_le_bytes([result[1], result[2], result[3], result[4]]);
                        let total =
                            u32::from_le_bytes([result[5], result[6], result[7], result[8]]);
                        debug!("OTA progress: {}/{} bytes", received, total);
                    }
                }
                b'C' => {
                    // Complete response
                    debug!("OTA complete, device will reboot");
                    return Ok(());
                }
                b'o' => {
                    // Ok response (alternative to Progress)
                    debug!("OTA chunk acknowledged");
                }
                b'e' => {
                    return Err(self.parse_ota_error(&result));
                }
                _ => {
                    return Err(Error::InvalidResponse);
                }
            }
        }

        // All data sent - the device should have sent OtaComplete
        // If we're here, something went wrong
        Err(Error::OtaError(
            "OTA did not complete as expected".to_string(),
        ))
    }

    /// Abort an in-progress OTA update.
    pub async fn abort_ota(&self) -> Result<(), Error> {
        self.write(b"A").await?;
        let result = self.read().await?;
        if result.len() != 1 || result[0] != b'o' {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }

    /// Query OTA progress.
    /// Returns (received_bytes, total_bytes) if OTA is in progress, None otherwise.
    pub async fn get_ota_progress(&self) -> Result<Option<(u32, u32)>, Error> {
        self.write(b"P").await?;
        let result = self.read().await?;
        if result.is_empty() {
            return Err(Error::InvalidResponse);
        }
        match result[0] {
            b'P' if result.len() >= 9 => {
                let received = u32::from_le_bytes([result[1], result[2], result[3], result[4]]);
                let total = u32::from_le_bytes([result[5], result[6], result[7], result[8]]);
                Ok(Some((received, total)))
            }
            b'o' => Ok(None), // Not in OTA mode
            b'e' => Err(self.parse_ota_error(&result)),
            _ => Err(Error::InvalidResponse),
        }
    }

    /// Parse OTA error response.
    fn parse_ota_error(&self, result: &[u8]) -> Error {
        if result.len() >= 3 && result[1] == b'O' {
            let error_msg = match result[2] {
                b'a' => "OTA already in progress",
                b'n' => "OTA not started",
                b'i' => "OTA initialization failed",
                b'w' => "OTA write failed",
                b'c' => "CRC mismatch",
                b'f' => "OTA flush failed",
                b's' => "Invalid firmware size",
                b'p' => "OTA partition not found",
                _ => "Unknown OTA error",
            };
            Error::OtaError(error_msg.to_string())
        } else {
            Error::InvalidResponse
        }
    }

    async fn try_open_device(di: DeviceInfo) -> Result<Self, Error> {
        let device = di.open().await?;
        let cfg = device.active_configuration()?;

        // Find the interface with class 0xFF, subclass 0x0D, and protocol 0x0A
        let iface_alt = cfg
            .interface_alt_settings()
            .find(|i| i.class() == 0xFF && i.subclass() == 0x0D && i.protocol() == 0x0A)
            .ok_or(Error::UnknownDevice)?;

        // Claim this interface
        let interface = device
            .claim_interface(iface_alt.interface_number())
            .await
            .map_err(|e| {
                if e.kind() == ErrorKind::PermissionDenied {
                    Error::PermissionDenied
                } else {
                    Error::DeviceBusy
                }
            })?;

        // Find the bulk IN and OUT endpoints
        let alt = interface.descriptors().next().ok_or(Error::UnknownDevice)?;
        let ep_in_addr = alt
            .endpoints()
            .find(|ep| ep.direction() == Direction::In)
            .ok_or(Error::UnknownDevice)?
            .address();
        let ep_out_addr = alt
            .endpoints()
            .find(|ep| ep.direction() == Direction::Out)
            .ok_or(Error::UnknownDevice)?
            .address();

        // Open the bulk endpoints
        let ep_in = interface.endpoint::<Bulk, In>(ep_in_addr)?;
        let ep_out = interface.endpoint::<Bulk, Out>(ep_out_addr)?;

        Ok(Self {
            device_info: di,
            ep_in: Mutex::new(ep_in),
            ep_out: Mutex::new(ep_out),
        })
    }

    async fn wait_for_device<A, B, C, D>(vid: A, pid: B, bus: C, address: D) -> Result<Self, Error>
    where
        A: Into<Option<u16>> + Clone,
        B: Into<Option<u16>> + Clone,
        C: Into<Option<String>> + Clone,
        D: Into<Option<u8>> + Clone,
    {
        // Create a watcher for hotplug events
        let mut watch = nusb::watch_devices().unwrap();

        // Check if the device is already connected
        let devices: Vec<DeviceInfo> = nusb::list_devices().await?.collect();
        for d in devices {
            if vid.clone().into().is_none_or(|v| d.vendor_id() == v)
                && pid.clone().into().is_none_or(|p| d.product_id() == p)
                && bus
                    .clone()
                    .into()
                    .is_none_or(|b| bus_id_matches(d.bus_id(), &b))
                && address
                    .clone()
                    .into()
                    .is_none_or(|a| d.device_address() == a)
            {
                loop {
                    match Self::try_open_device(d.clone()).await {
                        Ok(dev) => return Ok(dev),
                        Err(Error::DeviceBusy) => {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            continue;
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        // Wait for the device to be connected
        while let Some(event) = watch.next().await {
            if let HotplugEvent::Connected(di) = event {
                if vid.clone().into().is_none_or(|v| di.vendor_id() == v)
                    && pid.clone().into().is_none_or(|p| di.product_id() == p)
                    && bus
                        .clone()
                        .into()
                        .is_none_or(|b| bus_id_matches(di.bus_id(), &b))
                    && address
                        .clone()
                        .into()
                        .is_none_or(|a| di.device_address() == a)
                {
                    match Self::try_open_device(di).await {
                        Ok(dev) => return Ok(dev),
                        Err(_) => continue,
                    }
                }
            }
        }
        Err(Error::DeviceNotFound)
    }

    /// Write single packet to the device.
    /// The packet must be less than or equal to 64 bytes.
    async fn write(&self, data: &[u8]) -> Result<(), Error> {
        assert!(data.len() <= 64, "Buffer size must be less than or equal to 64 bytes");
        let mut buf = Buffer::new(64);
        buf.extend_from_slice(data);

        let mut ep_out = self.ep_out.lock().await;
        ep_out.submit(buf);
        let completion = ep_out.next_complete().await;
        completion.status.map_err(|e| e.into())
    }

    /// Read single packet from the device.
    async fn read(&self) -> Result<Vec<u8>, Error> {
        let buf = Buffer::new(64);

        let mut ep_in = self.ep_in.lock().await;
        ep_in.submit(buf);
        let completion = ep_in.next_complete().await;
        completion.status?;
        Ok(completion.buffer[..completion.actual_len].to_vec())
    }
}

/// Calculate CRC32 checksum (IEEE 802.3 polynomial).
/// This matches the CRC32 implementation in the firmware.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
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
        println!("{config:?}");
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_get_state() {
        let esparrier = Esparrier::auto_detect(false, None, None, None, None)
            .await
            .unwrap();
        let state = esparrier.get_state().await;
        println!("{state:?}");
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_get_config() {
        let esparrier = Esparrier::auto_detect(false, None, None, None, None)
            .await
            .unwrap();
        let config = esparrier.get_config().await.unwrap();
        println!("{config:?}");
    }

    #[ignore = "This test needs device attached"]
    #[tokio::test]
    async fn test_set_config() {
        let esparrier = Esparrier::auto_detect(false, None, None, None, None)
            .await
            .unwrap();
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
        let esparrier = Esparrier::auto_detect(false, None, None, None, None)
            .await
            .unwrap();
        let mut config = esparrier.get_config().await.unwrap();
        config.ssid = "test".to_string();
        esparrier.set_config(config).await.unwrap();
    }

    #[ignore = "This will reset the device"]
    #[tokio::test]
    async fn test_commit_config() {
        let esparrier = Esparrier::auto_detect(false, None, None, None, None)
            .await
            .unwrap();
        let mut config = esparrier.get_config().await.unwrap();
        config.ssid = "test".to_string();
        esparrier.set_config(config).await.unwrap();
        esparrier.commit_config().await.unwrap();
    }

    #[ignore = "This will reset the device"]
    #[tokio::test]
    async fn test_reboot() {
        let esparrier = Esparrier::auto_detect(false, None, None, None, None)
            .await
            .unwrap();
        esparrier.reboot_device().await.unwrap();
    }
}
