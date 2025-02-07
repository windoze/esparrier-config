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
  -w, --wait     Wait for the device to be connected
  -h, --help     Print help
  -V, --version  Print version
```

### Examples

* Get the device state:

    ```
    $ /path/to/esparrier-config-cli get-state
    {
        "version_major": 0,
        "version_minor": 4,
        "version_patch": 0,
        "feature_flags": 130,
        "ip_address": "192.168.1.123",
        "ip_prefix": 24,
        "server_connected": true,
        "active": false,
        "keep_awake": false
    }
    ```

* Get the device configuration:

    ```
    $ /path/to/esparrier-config-cli get-config
    {
        "ssid": "home-wifi",
        "server": "192.168.1.250:24800",
        "screen_name": "SCREEN1",
        "screen_width": 1920,
        "screen_height": 1080,
        "flip_wheel": false,
        "brightness": 10
    }
    ```

    NOTE: The Wi-Fi password is redacted for security reasons, so the output will not contain the `"password"` field thus cannot be used to set the configuration directly, you need to edit the configuration file manually or use `-p` option to read the password from the `WIFI_PASSWORD` environment variable when running `set-config`.

* Set the device configuration:

    * Prepare a JSON file with the new configuration, full format can be found at [config.json.example](https://github.com/windoze/esparrier/blob/main/config.json.example). For example:

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

        If the `-p` option is provided, the Wi-Fi password will be read from the `WIFI_PASSWORD` environment variable and the `"password"` field in the JSON file will be ignored thus can be omitted.
    
    * Set the new configuration:

        ```
        $ /path/to/esparrier-config-cli set-config /path/to/new-config.json
        ```
    
    * The device will restart and apply the new configuration. You can run `get-config` to verify the new configuration.

* Keep the computer awake:

    ```
    $ /path/to/esparrier-config-cli keep-awake
    ```

    The computer will not go to sleep as the device will keep sending mouse movement events to the host computer. The movement is smaller than a pixel so it is not noticeable.

    NOTE: This function is only effective when the device is connected to the Barrier/Deskflow server.

* Stop keeping the computer awake:

    ```
    $ /path/to/esparrier-config-cli no-keep-awake
    ```

    The device will stop sending mouse movement events so the computer will go to sleep after the configured time if there is no user activity.

## License

This project is licensed under the GPLv3 License - see the [LICENSE](LICENSE) file for details.
