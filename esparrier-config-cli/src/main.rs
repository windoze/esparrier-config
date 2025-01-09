use std::{io::Read, process::exit};

use clap::{Args, Parser, Subcommand};
use clap_num::maybe_hex;
use esparrier_config::Esparrier;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// USB Vendor ID
    #[clap(short, long, value_parser=maybe_hex::<u16>)]
    vid: Option<u16>,

    /// USB Product ID
    #[clap(short, long, value_parser=maybe_hex::<u16>)]
    pid: Option<u16>,

    /// USB bus number
    #[clap(short, long, value_parser=maybe_hex::<u8>)]
    bus: Option<u8>,

    /// USB device address
    #[clap(short, long, value_parser=maybe_hex::<u8>)]
    address: Option<u8>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Get device state
    GetState,
    /// Get device configuration
    GetConfig,
    /// Set device configuration
    SetConfig(SetConfigArgs),
    /// Commit the last configuration and restart the device
    CommitConfig,
}

#[derive(Debug, Args)]
struct SetConfigArgs {
    /// Path to the configuration file, if not provided, read from stdin
    #[clap(short, long)]
    filename: Option<String>,

    /// Commit the configuration to the device, the device will restart after commit
    #[clap(short, long, action, default_value = "false")]
    commit: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Some(esparrier) =
        esparrier_config::Esparrier::auto_detect(cli.vid, cli.pid, cli.bus, cli.address)
    {
        if let Err(e) = run_command(cli, esparrier).await {
            eprintln!("Error: {}", e);
            exit(1);
        }
    } else {
        eprintln!("No device found");
        exit(1);
    }
}

async fn run_command(cli: Cli, esparrier: Esparrier) -> anyhow::Result<()> {
    match cli.command {
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
            let config = serde_json::from_str(&content)?;
            esparrier.set_config(config).await?;
            if args.commit {
                esparrier.commit_config().await?;
                println!("Configuration committed, restarting device.");
            } else {
                println!("Configuration set, use `commit-config` to apply the configuration.");
            }
        }
        Commands::CommitConfig => {
            esparrier.commit_config().await?;
            println!("Configuration committed, restarting device.");
        }
    };
    Ok(())
}