use std::{io::Read, process::exit};

use clap::{Args, Command, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use clap_num::maybe_hex;
use esparrier_config::Esparrier;
use github_release::fetch_latest_release;

mod github_release;

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

    /// Optional, only look for devices with specified USB bus number
    #[clap(global = true, hide = true, long, value_parser=maybe_hex::<u8>)]
    bus: Option<u8>,

    /// Optional, only look for devices with specified USB device address
    #[clap(global = true, hide = true, long, value_parser=maybe_hex::<u8>)]
    address: Option<u8>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate shell completions
    Completions(GenerateArgs),
    /// Get device state, IP address, server connection status, etc.
    GetState,
    /// Get device configuration, secrets will be redacted
    GetConfig,
    /// Set device configuration
    SetConfig(SetConfigArgs),
    /// Commit the last configuration and restart the device
    CommitConfig,
    /// Enable keep awake
    KeepAwake,
    /// Disable keep awake
    NoKeepAwake,
    /// Reboot the device
    Reboot,
    /// Upgrade the device firmware
    UpgradeFirmware(UpgradeFirmwareArgs),
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

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

#[derive(Debug, Args)]
struct UpgradeFirmwareArgs {
    /// Path to the firmware bin file, omit to download the latest release from GitHub
    firmware: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Commands::Completions(args) = &cli.command {
        print_completions(args.shell, &mut Cli::command());
        return;
    }
    if let Some(esparrier) =
        esparrier_config::Esparrier::auto_detect(cli.wait, cli.vid, cli.pid, cli.bus, cli.address)
            .await
    {
        if let Err(e) = run_command(cli, esparrier).await {
            eprintln!("Error: {}", e);
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
        Commands::UpgradeFirmware(args) => {
            let bytes = match args.firmware {
                Some(firmware) => {
                    if !cli.quiet {
                        println!("Using firmware file: {}", firmware);
                    }
                    // Check if the file exists
                    if !std::path::Path::new(&firmware).exists() {
                        eprintln!("Firmware file does not exist: {}", firmware);
                        exit(1);
                    }
                    // Read the firmware file
                    let mut file = std::fs::File::open(&firmware)?;
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)?;
                    bytes
                }
                None => {
                    if !cli.quiet {
                        println!("No firmware file provided, downloading the latest release.");
                    }
                    let state = esparrier.get_state().await?;
                    let (tag_name, bytes) = fetch_latest_release(state.firmware_kind).await?;
                    if !cli.quiet {
                        println!("Latest release: {}", tag_name);
                    }
                    bytes
                }
            };
            esparrier.upgrade_firmware(&bytes).await?;
            if !cli.quiet {
                println!("Firmware upgrade started.");
            }
        }
    };
    Ok(())
}
