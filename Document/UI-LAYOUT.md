# Phira-Firefly UI 布局

## 概述

Phira-Firefly 在原版 Phira 基础上新增了 **UI 布局切换**功能，允许玩家在原版 UI 和 XCHS 风格 UI 之间切换。

- **Official**：原版 Phira UI（左侧竖排导航栏）
- **XCHS UI**：参考 XC-SIM 的粉色梅紫风格 UI

切换位置：**设置 → 常规 → UI 布局**，选择后弹窗提示重启游戏生效。

## 全局开关

- 全局原子：`prpr::ui::PREFER_XCHS_UI`（`prpr/src/ui.rs`）
- 数据字段：`phira/src/data.rs` 中 `ui_layout: usize`（0=Official, 1=XCHS）
- 启动时根据 `ui_layout` 初始化全局开关

## XCHS UI 配色

| 用途 | 颜色 | RGBA |
|---|---|---|
| 主题粉 accent | FIREFLY_PINK | (1.0, 0.58, 0.706, 1.0) |
| 深粉 | FIREFLY_PINK_DEEP | (0.949, 0.412, 0.580, 1.0) |
| 梅紫背景 | FIREFLY_PLUM | (0.196, 0.122, 0.196, 1.0) |
| 奶油软白 | FIREFLY_CREAM_SOFT | (0.984, 0.973, 0.886, 1.0) |
| 侧边栏底 | sidebar_bg | (0.15, 0.075, 0.12, 0.45) |
| 全局遮罩 | main_bg | (0.133, 0.071, 0.137, 0.92) |

## 已改造页面

### 主页（Home）
- 左侧导航卡宽度 0.46
- 大曲绘背景
- 底部粉色 "Phira-Firefly" 标题
- 右侧 credits 面板，文字为"小天是个小男娘"

### 选曲（Library）
- 顶部梅紫栏 + 粉彩带 + ♡ 装饰
- 左上粉色圆胶囊返回按钮
- "Library ♥" 标题 + 粉色下划线
- 右上 Order / Search / Import 按钮
- 左侧 side_w=0.44 竖排 6 个胶囊 tab（Local/Ranked/Special/Unstable/Popular/XcSim）
- 右侧大圆角内容卡片 + 粉色光晕描边
- 底部翻页胶囊条

### 设置（Settings）
- 左侧 side_w=0.52 竖排 5 个胶囊 tab
- 右侧梅紫内容卡片 + 章节标题栏 + 粉色分隔线

### 资源包（Respack）
- XCHS 3 列网格梅紫卡片（chh=0.14, gap=0.025）
- 选中粉色边框 + "♡ Active"
- 末尾 "＋ Import" 卡片

### 消息（Message）
- XCHS 3 列网格梅紫卡片（chh=0.18）
- 点卡片弹窗显示详情：梅紫背景 + 粉色头部 + ✕ 关闭按钮
- 弹窗内容可滚动（Scroll），scissor 裁剪
- 弹窗打开时拦截所有 touch，不穿透
- 弹窗打开时不渲染下方网格

### 全局（MainScene）
- 叠梅紫底 + 顶部粉彩带 + 上下 ♡ 装饰
- XCHS 模式跳过 fader title
- XCHS 模式下不画全局返回按钮（页面自绘）

## 构建

```bash
# 开发版（含测试功能）
cargo build --release --bin "phira-main" --features="phira/chat,phira/intest"

# 正式版
cargo build --release --bin "phira-main" --features="phira/chat"
```

产物：`target/release/phira-main.exe`
