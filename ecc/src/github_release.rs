use std::io::Read;

use esparrier_config::FirmwareKind;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

const GITHUB_API_URL: &str = "https://api.github.com/repos/windoze/esparrier/releases/latest";

fn match_firmware_kind(asset: &Asset, kind: FirmwareKind) -> bool {
    match kind {
        FirmwareKind::Generic => asset.name.starts_with("esparrier-generic-v"),
        FirmwareKind::M5AtomS3Lite => asset.name.starts_with("esparrier-m5atoms3-lite-v"),
        FirmwareKind::M5AtomS3 => asset.name.starts_with("esparrier-m5atoms3-v"),
        FirmwareKind::M5AtomS3R => asset.name.starts_with("esparrier-m5atoms3r-v"),
        FirmwareKind::XiaoESP32S3 => asset.name.starts_with("esparrier-xiao-esp32s3-v"),
        FirmwareKind::DevKitC1_0 => asset.name.starts_with("esparrier-devkitc-1_0-v"),
        FirmwareKind::DevKitC1_1 => asset.name.starts_with("esparrier-devkitc-1_1-v"),
        _ => false,
    }
}

pub async fn fetch_latest_release(kind: FirmwareKind) -> anyhow::Result<(String, Vec<u8>)> {
    if kind == FirmwareKind::Custom {
        return Err(anyhow::anyhow!("Custom firmware is not supported"));
    }

    let client = reqwest::Client::new();
    let response = client
        .get(GITHUB_API_URL)
        .header("User-Agent", format!("ecc/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await?
        .json::<GithubRelease>()
        .await?;

    // Find the asset that matches the firmware kind
    let asset = response
        .assets
        .iter()
        .find(|asset| match_firmware_kind(asset, kind))
        .ok_or_else(|| anyhow::anyhow!("No matching firmware found"))?;

    // Download the asset
    let firmware_response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", format!("ecc/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await?
        .bytes()
        .await?;

    // Extract `esparrier-xxx.bin` from the tar.gz file
    let tar_gz = flate2::read::GzDecoder::new(&firmware_response[..]);
    let mut archive = tar::Archive::new(tar_gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.starts_with("esparrier-") && entry.path()?.ends_with(".bin") {
            // Read the contents of the file into a vector
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer)?;
            return Ok((response.tag_name, buffer));
        }
    }

    Err(anyhow::anyhow!(
        "No valid firmware file found in the release"
    ))
}
