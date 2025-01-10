# Esparrier KVM Configuration Tool

This tool is designed to help you configure [Esparrier KVM](https://github.com/windoze/esparrier) devices.

## Building

1. Install Rust: https://www.rust-lang.org/tools/install
2. Clone this repository: `git clone https://github.com/windoze/esparrier-config.git`.
3. Change to the repository directory: `cd esparrier-config`.
4. Build the project: `cargo build --release`.

## Usage

Before running the tool, make sure the Esparrier KVM device is connected to the computer's USB port.

The tool is a command line application. Run it with the `help` sub-command to see the available options.

```
$ ./target/release/esparrier-config help
Configuration tools for Esparrier KVM devices

Usage: esparrier-config-cli <COMMAND>

Commands:
  get-state      Get device state, IP address, server connection status, etc
  get-config     Get device configuration, secrets will be redacted
  set-config     Set device configuration
  commit-config  Commit the last configuration and restart the device
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Examples

* Get the device state:

    ```
    $ /path/to/esparrier-config-cli get-state
    {
        "version_major": 0,
        "version_minor": 3,
        "version_patch": 1,
        "feature_flags": 130,
        "ip_address": "192.168.1.123",
        "ip_prefix": 24,
        "server_connected": true,
        "active": false
    }
    ```

* Get the device configuration:

    ```
    # NOTE: The Wi-Fi password is redacted
    $ /path/to/esparrier-config-cli get-config
    {
        "ssid": "home-wifi",
        "server": "192.168.1.250:24800",
        "screen_name": "SCREEN1",
        "screen_width": 1920,
        "screen_height": 1080,
        "flip_wheel": false,
        "brightness": 10
    }```

* Set the device configuration:

    * Prepare a JSON file with the new configuration, full format can be found at [config.rs.example](https://github.com/windoze/esparrier/blob/main/config.json.example). For example:

        ```json
        {
            "ssid": "home-wifi",
            "password": "home-wifi-password",
            "server": "192.168.1.250:24800",
            "screen_name": "SCREEN1",
            "screen_width": 1920,
            "screen_height": 1080,
            "flip_wheel": false,
            "brightness": 10
        }
        ```
    
    * Set the new configuration:

        ```
        $ /path/to/esparrier-config-cli set-config /path/to/new-config.json
        ```
    
    * Commit the new configuration:

        ```
        $ /path/to/esparrier-config-cli commit-config
        ```
    
    * The device will restart and apply the new configuration. You can run `get-config` to verify the new configuration.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
