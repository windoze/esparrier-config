[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/windoze/esparrier-config)

# Esparrier KVM 配置工具

本工具用于配置 [Esparrier KVM](https://github.com/windoze/esparrier) 设备。

## 构建

1. 安装 Rust：https://www.rust-lang.org/tools/install
2. 克隆本仓库：`git clone https://github.com/windoze/esparrier-config.git`
3. 进入仓库目录：`cd esparrier-config`
4. 构建项目：`cargo build --release`

## 使用方法

运行工具前，请确保 Esparrier KVM 设备已连接到计算机的 USB 端口。

### Linux udev 规则 (Ubuntu)

在 Linux 上，您可能需要设置 udev 规则以允许非 root 用户访问 USB 设备。创建文件 `/etc/udev/rules.d/99-esparrier.rules`，内容如下：

```
SUBSYSTEM=="usb", ATTR{idVendor}=="0d0a", ATTR{idProduct}=="c0de", MODE="0666"
```

然后重新加载 udev 规则：

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

您可能需要拔出并重新插入设备才能使新规则生效。

### 命令行界面

本工具是一个命令行应用程序。使用 `help` 子命令查看可用选项。

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

### 示例

* 获取设备状态：

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

* 获取设备配置：

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

    注意：出于安全考虑，Wi-Fi 密码会被隐藏，因此输出中不包含 `"password"` 字段，不能直接用于设置配置。您需要手动编辑配置文件，或在运行 `set-config` 时使用 `-p` 选项从 `WIFI_PASSWORD` 环境变量读取密码。

* 设置设备配置：

    * 准备一个包含新配置的 JSON 文件，完整格式请参见 [config.json.example](https://github.com/windoze/esparrier/blob/main/config.json.example)。例如：

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

        如果提供了 `-p` 选项，Wi-Fi 密码将从 `WIFI_PASSWORD` 环境变量读取，JSON 文件中的 `"password"` 字段将被忽略，因此可以省略。

    * 设置新配置：

        ```
        $ /path/to/ecc set-config /path/to/new-config.json
        ```

    * 设备将重启并应用新配置。您可以运行 `get-config` 来验证新配置。

* 保持计算机唤醒：

    ```
    $ /path/to/ecc keep-awake
    ```

    设备会持续向主机发送鼠标移动事件，使计算机不会进入睡眠状态。移动幅度小于一个像素，因此不会被察觉。

    注意：此功能仅在设备连接到 Barrier/Deskflow 服务器时有效。

* 停止保持计算机唤醒：

    ```
    $ /path/to/ecc no-keep-awake
    ```

    设备将停止发送鼠标移动事件，如果没有用户活动，计算机将在配置的时间后进入睡眠状态。

* 通过 OTA 更新固件：

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

    工具会根据设备型号自动从 GitHub releases 下载最新固件。使用 `--force` 可以重新安装相同版本或降级，使用 `--file` 可以指定本地固件文件。

    注意：OTA 需要固件启用 OTA 功能。如果您的设备不支持 OTA，需要手动刷写固件。

## 许可证

本项目采用 GPLv3 许可证 - 详情请参见 [LICENSE](LICENSE) 文件。
