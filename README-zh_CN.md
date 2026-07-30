# Phira-Firefly

![Phira-Firefly](https://raw.githubusercontent.com/tiancra/Phira-Firefly/main/assets/icon.png)

[English](./README.md)

Phira-Firefly 是 [Phira](https://github.com/TeamFlos/phira) 的分支，一款受 Phigros 启发的跨平台音乐节奏游戏，使用 Rust 开发。

## 特性

- 包含原版 Phira 的全部功能
- **跳过曲目（Track Skip）**：通过 Alt+S 或四指按屏幕四角跳过谱面
- **动态背景**：基于曲绘代表色实时演算的弥散流体背景
- **歌词支持**：基于 TTML 的游玩内歌词显示
- **新手教程**：内置新手引导教程
- **崩溃界面**：自定义崩溃提示界面
- **文言本地化**：完整的 zh-LZH 文言文翻译支持
- **界面增强**：自定义窗口标题、图标及多项界面改进

## 下载

- [GitHub Release](https://github.com/tiancra/Phira-Firefly/releases): 安卓、Windows、Linux

## 从源码构建

### 前置要求

- [Rust](https://rustup.rs/) 工具链
- Android 构建需安装 Android NDK

### 构建

```bash
# 克隆仓库
git clone https://github.com/tiancra/Phira-Firefly.git
cd Phira-Firefly

# 桌面版构建
cargo build --release -p phira-main

# Android 构建
# 参考 build-android.ps1
```

## 贡献

欢迎提交 Issue 和 Pull Request！

## 翻译

项目支持多种语言，包括英语、简体中文、繁体中文、文言、日语、韩语、俄语、法语等。

## 致谢

- [Phira](https://github.com/TeamFlos/phira) - 由 TeamFlos 开发的原项目
- [Phigros](https://phigros.com/) - 本游戏的灵感来源

## 许可证

[GPL-3.0](./LICENSE)

## Star 历史

[![Stargazers over time](https://starchart.cc/tiancra/Phira-Firefly.svg?variant=adaptive)](https://starchart.cc/tiancra/Phira-Firefly)
