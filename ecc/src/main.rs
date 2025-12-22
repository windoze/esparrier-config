use std::{io::Read, process::exit};

use clap::{Args, Command, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use clap_num::maybe_hex;
use esparrier_config::Esparrier;
use semver::Version;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Wait for the device to be connected
    #[clap(global = true, short, long, action, default_value = "false")]
    wait: bool,

    /// Quiet mode, do not print any non-error messages
    #[clap(global = true, short, long, action, default_value = "false")]
    quiet: bool,

    /// Optional, only look for devices with specified USB Vendor ID
    #[clap(global = true, hide = true, long, value_parser=maybe_hex::<u16>)]
    vid: Option<u16>,

    /// Optional, only look for devices with specified USB Product ID
    #[clap(global = true, hide = true, long, value_parser=maybe_hex::<u16>)]
    pid: Option<u16>,

    /// Optional, only look for devices with specified USB bus ID
    #[clap(global = true, long)]
    bus: Option<String>,

    /// Optional, only look for devices with specified USB device address
    #[clap(global = true, long, value_parser=maybe_hex::<u8>)]
    address: Option<u8>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate shell completions
    Completions(GenerateArgs),
    /// List available devices
    List,
    /// Get device state, IP address, server connection status, etc.
    GetState,
    /// Get device configuration, secrets will be redacted
    GetConfig,
    /// Set device configuration
    SetConfig(SetConfigArgs),
    /// Commit the last configuration and restart the device
    #[clap(hide = true)]
    CommitConfig,
    /// Enable keep awake
    KeepAwake,
    /// Disable keep awake
    NoKeepAwake,
    /// Reboot the device
    Reboot,
    /// Upload firmware via OTA (Over-The-Air update)
    Ota(OtaArgs),
}

#[derive(Debug, Args)]
struct GenerateArgs {
    /// Shell to generate completions for
    shell: Shell,
}

#[derive(Debug, Args)]
struct SetConfigArgs {
    /// Path to the configuration file, if not provided, read from stdin
    filename: Option<String>,

    /// Set WiFi name from the `WIFI_SSID` environment variable
    #[clap(short = 's', long, action, default_value = "false")]
    use_env_wifi_ssid: bool,

    /// Set WiFi password from the `WIFI_PASSWORD` environment variable
    #[clap(short = 'p', long, action, default_value = "false")]
    use_env_wifi_password: bool,

    /// Do not commit the configuration to the device
    #[clap(long, action, hide = true, default_value = "false")]
    no_commit: bool,
}

#[derive(Debug, Args)]
struct OtaArgs {
    /// Path to local firmware binary file (if not provided, downloads from GitHub)
    #[clap(short, long)]
    file: Option<String>,

    /// Force update even if versions match or downgrading
    #[clap(short = 'F', long, action, default_value = "false")]
    force: bool,

    /// Skip version check (only applies to remote downloads)
    #[clap(long, action, default_value = "false")]
    skip_version_check: bool,
}

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = Cli::parse();
    if let Commands::Completions(args) = &cli.command {
        print_completions(args.shell, &mut Cli::command());
        return;
    }
    if let Some(esparrier) =
        esparrier_config::Esparrier::auto_detect(cli.wait, cli.vid, cli.pid, cli.bus.clone(), cli.address)
            .await
    {
        if let Err(e) = run_command(cli, esparrier).await {
            eprintln!("Error: {e}");
            exit(1);
        }
    } else {
        eprintln!("Esparrier KVM not found");
        exit(1);
    }
}

async fn run_command(cli: Cli, esparrier: Esparrier) -> anyhow::Result<()> {
    match cli.command {
        Commands::Completions(_args) => {
            unreachable!("Generate command should have been handled in main()");
        }
        Commands::List => {
            let devices = esparrier_config::Esparrier::list_devices(cli.vid, cli.pid).await;
            if devices.is_empty() {
                if !cli.quiet {
                    println!("No Esparrier KVM devices found.");
                }
            } else {
                println!("Found {} Esparrier KVM devices:", devices.len());
                for (idx, (bus, address)) in devices.iter().enumerate() {
                    println!("{}: Bus: {}, Address: {}", idx + 1, bus, address);
                }
            }
        }
        Commands::GetState => {
            let state = esparrier.get_state().await?;
            println!("{}", serde_json::to_string_pretty(&state)?);
        }
        Commands::GetConfig => {
            let config = esparrier.get_config().await?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        Commands::SetConfig(args) => {
            let content = match args.filename {
                Some(filename) => {
                    let mut file = std::fs::File::open(filename)?;
                    let mut content = String::new();
                    file.read_to_string(&mut content)?;
                    content
                }
                None => {
                    let mut content = String::new();
                    std::io::stdin().read_to_string(&mut content)?;
                    content
                }
            };
            let mut config: esparrier_config::EsparrierConfig = serde_json::from_str(&content)?;
            if args.use_env_wifi_ssid {
                if let Ok(wifi_ssid) = std::env::var("WIFI_SSID") {
                    config.ssid = wifi_ssid;
                }
            }
            if args.use_env_wifi_password {
                if let Ok(wifi_password) = std::env::var("WIFI_PASSWORD") {
                    config.password = wifi_password;
                }
            }
            esparrier.set_config(config).await?;
            if args.no_commit {
                if !cli.quiet {
                    println!("Configuration set, use `commit-config` to apply the configuration.");
                }
            } else {
                esparrier.commit_config().await?;
                if !cli.quiet {
                    println!("Configuration committed, restarting device.");
                }
            }
        }
        Commands::CommitConfig => {
            esparrier.commit_config().await?;
            if !cli.quiet {
                println!("Configuration committed, restarting device.");
            }
        }
        Commands::KeepAwake => {
            esparrier.keep_awake(true).await?;
            if !cli.quiet {
                println!("Computer will stay awake.");
            }
        }
        Commands::NoKeepAwake => {
            esparrier.keep_awake(false).await?;
            if !cli.quiet {
                println!("Computer will not stay awake.");
            }
        }
        Commands::Reboot => {
            esparrier.reboot_device().await?;
            if !cli.quiet {
                println!("Device rebooted.");
            }
        }
        Commands::Ota(args) => {
            // First check if OTA is supported
            let state = esparrier.get_state().await?;
            if !state.has_ota_support() {
                anyhow::bail!("OTA is not supported by this firmware. Please update the firmware with OTA feature enabled.");
            }

            let firmware = if let Some(ref filename) = args.file {
                // Local file mode
                let firmware = std::fs::read(filename)?;
                if !cli.quiet {
                    println!("Uploading firmware from local file: {} ({} bytes)", filename, firmware.len());
                }
                firmware
            } else {
                // Remote download mode (default)
                let model_name = state.model_name().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Unknown device model (id={}). Use --file to specify a local firmware file.",
                        state.model_id
                    )
                })?;

                if !cli.quiet {
                    println!("Device: {} (model_id={})", model_name, state.model_id);
                    println!("Current firmware version: {}", state.version_string());
                    println!("Checking for latest release...");
                }

                // Get release info first (without downloading)
                let release_info = get_firmware_release_info(model_name).await?;

                if !cli.quiet {
                    println!("Latest release: {}", release_info.tag_name);
                }

                // Version check before downloading
                if !args.skip_version_check && !args.force {
                    let current_version = Version::new(
                        state.version_major as u64,
                        state.version_minor as u64,
                        state.version_patch as u64,
                    );

                    if release_info.version <= current_version {
                        if release_info.version == current_version {
                            anyhow::bail!(
                                "Device is already running version {}. Use --force to reinstall.",
                                state.version_string()
                            );
                        } else {
                            anyhow::bail!(
                                "Release version {} is older than current version {}. Use --force to downgrade.",
                                release_info.version,
                                state.version_string()
                            );
                        }
                    }

                    if !cli.quiet {
                        println!(
                            "Updating from {} to {}",
                            state.version_string(),
                            release_info.version
                        );
                    }
                }

                // Now download the firmware
                download_firmware(&release_info.asset, cli.quiet).await?
            };

            // Upload with progress callback
            let quiet = cli.quiet;
            esparrier
                .upload_ota(
                    &firmware,
                    Some(|received: usize, total: usize| {
                        if !quiet {
                            let percent = (received * 100) / total;
                            eprint!("\rProgress: {}% ({}/{} bytes)", percent, received, total);
                        }
                    }),
                )
                .await?;

            if !cli.quiet {
                eprintln!(); // New line after progress
                println!("OTA complete! Device is rebooting with new firmware.");
            }
        }
    };
    Ok(())
}

const GITHUB_RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/windoze/esparrier/releases/latest";
const GITHUB_RELEASES_BY_TAG_URL: &str =
    "https://api.github.com/repos/windoze/esparrier/releases/tags";

#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GitHubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

/// Information about a firmware release, retrieved before downloading.
struct FirmwareReleaseInfo {
    version: Version,
    tag_name: String,
    asset: GitHubAsset,
}

/// Get firmware release info from GitHub without downloading.
/// Returns version and asset info for the specified model.
async fn get_firmware_release_info(model_name: &str) -> anyhow::Result<FirmwareReleaseInfo> {
    let client = reqwest::Client::builder()
        .user_agent("esparrier-config-cli")
        .build()?;

    // Fetch latest release info to get the version tag
    let latest_release: GitHubRelease = client
        .get(GITHUB_RELEASES_LATEST_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let tag_name = latest_release.tag_name;

    // Parse version from tag (e.g., "v0.7.0" -> "0.7.0")
    let version_str = tag_name.strip_prefix('v').unwrap_or(&tag_name);
    let version = Version::parse(version_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse release version '{}': {}", version_str, e))?;

    // Fetch full release info by tag (this returns all assets)
    let release: GitHubRelease = client
        .get(format!("{}/{}", GITHUB_RELEASES_BY_TAG_URL, tag_name))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // Find the asset for this model
    let asset_prefix = format!("esparrier-{}-v", model_name);
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name.starts_with(&asset_prefix) && a.name.ends_with(".tar.gz"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No firmware found for model '{}' in release {}",
                model_name,
                tag_name
            )
        })?;

    Ok(FirmwareReleaseInfo {
        version,
        tag_name,
        asset,
    })
}

/// Download and extract firmware from a GitHub release asset.
async fn download_firmware(asset: &GitHubAsset, quiet: bool) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent("esparrier-config-cli")
        .build()?;

    if !quiet {
        println!("Downloading: {} ({} bytes)", asset.name, asset.size);
    }

    // Download the tarball with progress
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()?;

    let total_size = asset.size;
    let mut downloaded: u64 = 0;
    let mut tarball_bytes = Vec::with_capacity(total_size as usize);

    use futures::StreamExt;
    let mut stream = response.bytes_stream();
    while let Some(result) = stream.next().await {
        let chunk = result?;
        downloaded += chunk.len() as u64;
        tarball_bytes.extend_from_slice(&chunk);
        if !quiet {
            let percent = (downloaded * 100) / total_size;
            eprint!("\rDownload progress: {}% ({}/{} bytes)", percent, downloaded, total_size);
        }
    }
    if !quiet {
        eprintln!(); // New line after progress
        println!("Extracting firmware...");
    }

    // Extract the .bin file from the tarball
    let firmware = extract_firmware_from_tarball(&tarball_bytes)?;

    if !quiet {
        println!("Firmware size: {} bytes", firmware.len());
    }

    Ok(firmware)
}

/// Extract the firmware .bin file from a tar.gz archive.
fn extract_firmware_from_tarball(tarball_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Cursor;
    use tar::Archive;

    let cursor = Cursor::new(tarball_bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let path_str = path.to_string_lossy();

        // Look for the OTA binary file (esparrier-*.bin, not merged-*.bin)
        // merged-*.bin is the full flash image, esparrier-*.bin is the OTA-compatible firmware
        if path_str.ends_with(".bin")
            && !path_str.contains("bootloader")
            && !path_str.contains("partition")
            && !path_str.contains("merged")
        {
            let mut firmware = Vec::new();
            entry.read_to_end(&mut firmware)?;
            return Ok(firmware);
        }
    }

    anyhow::bail!("No firmware .bin file found in the archive")
}
