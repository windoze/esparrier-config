[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/windoze/esparrier-config)

# Esparrier KVM Configuration Tool

This tool is designed to help you configure [Esparrier KVM](https://github.com/windoze/esparrier) devices.

## Building

1. Install Rust: https://www.rust-lang.org/tools/install
2. Clone this repository: `git clone https://github.com/windoze/esparrier-config.git`.
3. Change to the repository directory: `cd esparrier-config`.
4. Build the project: `cargo build --release`.

## Usage

Before running the tool, make sure the Esparrier KVM device is connected to the computer's USB port.

### Linux udev Rules (Ubuntu)

On Linux, you may need to set up udev rules to allow non-root users to access the USB device. Create a file `/etc/udev/rules.d/99-esparrier.rules` with the following content:

```
SUBSYSTEM=="usb", ATTR{idVendor}=="0d0a", ATTR{idProduct}=="c0de", MODE="0666"
```

Then reload the udev rules:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

You may need to unplug and replug the device for the new rules to take effect.

### Command Line Interface

The tool is a command line application. Run it with the `help` sub-command to see the available options.

```
$ /path/to/ecc help
Configuration tools for Esparrier KVM devices

Usage: ecc [OPTIONS] <COMMAND>

Commands:
  completions    Generate shell completions
  list           List available devices
  get-state      Get device state, IP address, server connection status, etc
  get-config     Get device configuration, secrets will be redacted
  set-config     Set device configuration
  keep-awake     Enable keep awake
  no-keep-awake  Disable keep awake
  reboot         Reboot the device
  ota            Upload firmware via OTA (Over-The-Air update)
  help           Print this message or the help of the given subcommand(s)

Options:
  -w, --wait     Wait for the device to be connected
  -h, --help     Print help
  -V, --version  Print version
```

### Examples

* Get the device state:

    ```
    $ /path/to/ecc get-state
    {
        "version_major": 0,
        "version_minor": 5,
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
    $ /path/to/ecc get-config
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
        $ /path/to/ecc set-config /path/to/new-config.json
        ```
    
    * The device will restart and apply the new configuration. You can run `get-config` to verify the new configuration.

* Keep the computer awake:

    ```
    $ /path/to/ecc keep-awake
    ```

    The computer will not go to sleep as the device will keep sending mouse movement events to the host computer. The movement is smaller than a pixel so it is not noticeable.

    NOTE: This function is only effective when the device is connected to the Barrier/Deskflow server.

* Stop keeping the computer awake:

    ```
    $ /path/to/ecc no-keep-awake
    ```

    The device will stop sending mouse movement events so the computer will go to sleep after the configured time if there is no user activity.

* Update firmware via OTA:

    ```
    $ /path/to/ecc ota
    Device: m5atoms3 (model_id=2)
    Current firmware version: 0.7.0
    Checking for latest release...
    Latest release: v0.9.0
    Updating from 0.7.0 to 0.9.0
    Downloading: esparrier-m5atoms3-v0.9.0.tar.gz (654321 bytes)
    Download progress: 100% (654321/654321 bytes)
    Extracting firmware...
    Firmware size: 524288 bytes
    Progress: 100% (524288/524288 bytes)
    OTA complete! Device is rebooting with new firmware.
    ```

    The tool automatically downloads the latest firmware from GitHub releases based on the device model. Use `--force` to reinstall the same version or downgrade, or `--file` to specify a local firmware file.

    NOTE: OTA requires firmware with OTA feature enabled. If your device doesn't support OTA, you'll need to flash the firmware manually.

## License

This project is licensed under the GPLv3 License - see the [LICENSE](LICENSE) file for details.
