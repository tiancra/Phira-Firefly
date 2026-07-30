# Phira-Firefly

![Phira-Firefly](https://raw.githubusercontent.com/tiancra/Phira-Firefly/main/assets/icon.png)

[中文版本](./README-zh_CN.md)

Phira-Firefly is a fork of [Phira](https://github.com/TeamFlos/phira), a cross-platform rhythm game inspired by Phigros, developed with Rust.

## Features

- All features from the original Phira
- **Track Skip**: Skip charts via Alt+S or four-finger corner press
- **Dynamic Background**: Real-time computed diffuse fluid background based on chart cover colors
- **Lyrics Support**: TTML-based lyrics display during gameplay
- **Tutorial**: Built-in beginner tutorial
- **Crash Screen**: Custom crash screen with error details
- **文言 (Classical Chinese) Locale**: Full localization support for zh-LZH
- **UI Enhancements**: Custom window title, icon, and various interface improvements

## Download

- [GitHub Release](https://github.com/tiancra/Phira-Firefly/releases): For Android, Windows and Linux

## Build from Source

### Prerequisites

- [Rust](https://rustup.rs/) toolchain
- For Android: Android NDK

### Build

```bash
# Clone the repository
git clone https://github.com/tiancra/Phira-Firefly.git
cd Phira-Firefly

# Build for desktop
cargo build --release -p phira-main

# Build for Android
# See build-android.ps1 for reference
```

## Contribution

Issues & pull requests are welcome!

## Translation

The project supports multiple languages including English, Simplified Chinese, Traditional Chinese, Classical Chinese (文言), Japanese, Korean, Russian, French, and more.

## Acknowledgements

- [Phira](https://github.com/TeamFlos/phira) - The original project by TeamFlos
- [Phigros](https://phigros.com/) - The inspiration for this game

## License

[GPL-3.0](./LICENSE)

## Star History

[![Stargazers over time](https://starchart.cc/tiancra/Phira-Firefly.svg?variant=adaptive)](https://starchart.cc/tiancra/Phira-Firefly)
