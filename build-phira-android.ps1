# Phira / Phira-Firefly Android ARM64 Build Script
# Windows PowerShell 5.1 / PowerShell 7
# Usage:
#   Set-ExecutionPolicy -Scope Process Bypass
#   .\build-phira-android.ps1
#
# Optional:
#   .\build-phira-android.ps1 -ProjectPath "D:\Phira-Firefly"
#   .\build-phira-android.ps1 -NdkVersion "27.2.12479018"
#
# Notes:
# - Builds the Rust Android native library for arm64-v8a.
# - This script does NOT modify your source code.
# - prpr-avc/static-lib must already exist in the source tree if your branch requires it.

[CmdletBinding()]
param(
    [string]$ProjectPath = (Get-Location).Path,
    [string]$NdkVersion = "27.2.12479018",
    [int]$AndroidApi = 35,
    [switch]$NoInstall
)

$ErrorActionPreference = "Stop"

function Step($Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Fail($Message) {
    Write-Host "`n[ERROR] $Message" -ForegroundColor Red
    exit 1
}

function Check-Command($Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "$Name 未找到。请先安装对应工具。"
    }
}

Step "检查项目目录"
$ProjectPath = (Resolve-Path $ProjectPath).Path
Set-Location $ProjectPath

if (-not (Test-Path ".\Cargo.toml")) {
    Fail "当前目录不是 Phira/Rust 项目根目录：$ProjectPath"
}

Write-Host "项目目录: $ProjectPath"

Step "检查 Git / Rust"
Check-Command git
Check-Command rustc
Check-Command cargo
Check-Command rustup

rustc -V
cargo -V

Step "检查 Rust target"
$Target = "aarch64-linux-android"
$installedTargets = rustup target list --installed
if ($installedTargets -contains $Target) {
    Write-Host "$Target 已安装，跳过。" -ForegroundColor Green
} else {
    if ($NoInstall) {
        Fail "缺少 Rust target $Target。请运行：rustup target add $Target"
    }
    Write-Host "未安装 $Target，开始安装..."
    rustup target add $Target
}

Step "检查 Android SDK"
$Sdk = $env:ANDROID_HOME
if ([string]::IsNullOrWhiteSpace($Sdk)) {
    $Sdk = $env:ANDROID_SDK_ROOT
}
if ([string]::IsNullOrWhiteSpace($Sdk)) {
    $Sdk = "D:\Android\SDK"
}
if (-not (Test-Path $Sdk)) {
    $Sdk = Join-Path $env:LOCALAPPDATA "Android\Sdk"
}

if (-not (Test-Path $Sdk)) {
    Fail "找不到 Android SDK：$Sdk。请先安装 Android Studio/Android SDK。"
}

$env:ANDROID_HOME = $Sdk
$env:ANDROID_SDK_ROOT = $Sdk
Write-Host "ANDROID_HOME = $Sdk"

Step "检查 Android NDK"
$NdkRoot = Join-Path $Sdk "ndk\$NdkVersion"

if (Test-Path $NdkRoot) {
    Write-Host "NDK $NdkVersion 已安装，跳过安装。" -ForegroundColor Green
} else {
    Write-Host "未找到 NDK $NdkVersion：$NdkRoot" -ForegroundColor Yellow

    $sdkmanager = Join-Path $Sdk "cmdline-tools\latest\bin\sdkmanager.bat"
    if (-not (Test-Path $sdkmanager)) {
        $sdkmanager = Join-Path $Sdk "cmdline-tools\bin\sdkmanager.bat"
    }

    if ((-not $NoInstall) -and (Test-Path $sdkmanager)) {
        Write-Host "尝试通过 sdkmanager 安装 NDK $NdkVersion ..."
        & $sdkmanager "ndk;$NdkVersion"
        if ($LASTEXITCODE -ne 0) {
            Fail "sdkmanager 安装 NDK 失败。请在 Android Studio 的 SDK Manager 中手动安装 NDK $NdkVersion。"
        }
    }
}

if (-not (Test-Path $NdkRoot)) {
    Fail "仍然找不到 NDK $NdkVersion。路径应为：$NdkRoot"
}

$env:ANDROID_NDK_HOME = $NdkRoot
$env:ANDROID_NDK_ROOT = $NdkRoot
Write-Host "ANDROID_NDK_HOME = $NdkRoot"

Step "检查 cargo-ndk"
if (Get-Command cargo-ndk -ErrorAction SilentlyContinue) {
    Write-Host "cargo-ndk 已安装，跳过安装。" -ForegroundColor Green
} else {
    if ($NoInstall) {
        Fail "未找到 cargo-ndk。请运行：cargo install cargo-ndk"
    }

    Write-Host "未安装 cargo-ndk，开始安装..."
    cargo install cargo-ndk
    if ($LASTEXITCODE -ne 0) {
        Fail "cargo-ndk 安装失败。"
    }
}

cargo ndk --version

Step "检查 prpr-avc 静态库"
$AvcStatic = Join-Path $ProjectPath "prpr-avc\static-lib"

if (-not (Test-Path $AvcStatic)) {
    Write-Host "警告：没有找到 prpr-avc\static-lib。" -ForegroundColor Yellow
    Write-Host "如果你的源码版本需要预编译 FFmpeg 静态库，请先按照构建文档下载并解压 prpr-avc.zip。"
    Write-Host "当前脚本不会擅自下载未知版本的 FFmpeg 静态库。"
}

Step "检查 local_key_cell_methods"
$PrprLib = Join-Path $ProjectPath "prpr\src\lib.rs"
if (Test-Path $PrprLib) {
    $content = Get-Content $PrprLib -Raw
    if ($content -match '#!\[feature\(local_key_cell_methods\)\]') {
        Write-Host "发现 local_key_cell_methods feature。" -ForegroundColor Yellow
        Write-Host "不会自动修改源码；如果你的 Rust 版本不接受该 feature，请按对应分支的构建要求处理。"
    }
}

Step "开始 Android ARM64 Release 构建"
Write-Host "Target : $Target"
Write-Host "API    : $AndroidApi"
Write-Host "Mode   : release"

cargo ndk -t arm64-v8a --platform $AndroidApi build --release
if ($LASTEXITCODE -ne 0) {
    Fail "cargo ndk 构建失败。请把上面的完整错误日志发给我。"
}

Step "查找生成的 libphira.so"
$candidates = @(
    (Join-Path $ProjectPath "target\aarch64-linux-android\release\libphira.so"),
    (Join-Path $ProjectPath "target\aarch64-linux-android\release\libphira.dll")
)

$Found = $null
foreach ($p in $candidates) {
    if (Test-Path $p) {
        $Found = $p
        break
    }
}

if ($null -eq $Found) {
    $Found = Get-ChildItem -Path (Join-Path $ProjectPath "target") -Filter "libphira.so" -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}

if ($null -eq $Found) {
    Write-Host "`n构建命令成功返回，但没有自动找到 libphira.so。" -ForegroundColor Yellow
    Write-Host "请检查 target 目录。"
    exit 0
}

Write-Host "`n========================================" -ForegroundColor Green
Write-Host "Android ARM64 构建成功！" -ForegroundColor Green
Write-Host "libphira.so:" -ForegroundColor Green
Write-Host $Found -ForegroundColor White
Write-Host "========================================" -ForegroundColor Green
