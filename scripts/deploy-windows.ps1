# PowerShell Deployment Script
# Usage: .\scripts\deploy-windows.ps1

param(
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

# Directories
$RootDir = Split-Path -Parent $PSScriptRoot
$BinDir = Join-Path $RootDir "bin"
$BinPluginsDir = Join-Path $BinDir "plugins"
$BinConfigsDir = Join-Path $BinDir "configs"
$TargetDir = Join-Path $RootDir "target\$Profile"
$ConfigsDir = Join-Path $RootDir "configs"

Write-Host "=== Log Analyzer Deployment ===" -ForegroundColor Cyan
Write-Host "Profile: $Profile" -ForegroundColor Green
Write-Host ""

# Step 1: Clean old bin directory
Write-Host "[1/5] Cleaning old bin directory..." -ForegroundColor Yellow
if (Test-Path $BinDir) {
    Remove-Item -Recurse -Force $BinDir
    Write-Host "  Removed: $BinDir" -ForegroundColor Gray
}

# Step 2: Create directory structure
Write-Host ""
Write-Host "[2/5] Creating directory structure..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $BinPluginsDir | Out-Null
New-Item -ItemType Directory -Force -Path $BinConfigsDir | Out-Null
Write-Host "  Created: bin/" -ForegroundColor Gray
Write-Host "  Created: bin/plugins/" -ForegroundColor Gray
Write-Host "  Created: bin/configs/" -ForegroundColor Gray

# Step 3: Copy executable
Write-Host ""
Write-Host "[3/5] Copying executable..." -ForegroundColor Yellow
$ExeName = "analyzer.exe"
$ExePath = Join-Path $TargetDir $ExeName
if (Test-Path $ExePath) {
    Copy-Item $ExePath -Destination $BinDir
    Write-Host "  Copied: $ExeName" -ForegroundColor Green
} else {
    Write-Host "  ERROR: Executable not found: $ExePath" -ForegroundColor Red
    Write-Host "  Please run: cargo build --release" -ForegroundColor Yellow
    exit 1
}

# Step 4: Copy plugins
Write-Host ""
Write-Host "[4/5] Copying plugins..." -ForegroundColor Yellow
$PluginNames = @(
    "master_control_analyzer.dll",
    "cpp_demo_analyzer.dll"
)
$PluginCount = 0
foreach ($PluginName in $PluginNames) {
    $PluginPath = Join-Path $TargetDir $PluginName
    if (Test-Path $PluginPath) {
        Copy-Item $PluginPath -Destination $BinPluginsDir
        Write-Host "  Copied: $PluginName" -ForegroundColor Green
        $PluginCount++
    } else {
        Write-Host "  Warning: Plugin not found: $PluginName" -ForegroundColor DarkYellow
    }
}
Write-Host "  Successfully copied $PluginCount plugin(s)" -ForegroundColor Green

# Step 5: Copy configs
Write-Host ""
Write-Host "[5/5] Copying config files..." -ForegroundColor Yellow
if (Test-Path $ConfigsDir) {
    Copy-Item -Path "$ConfigsDir\*" -Destination $BinConfigsDir -Recurse
    $ConfigFiles = Get-ChildItem -Path $BinConfigsDir -File
    foreach ($ConfigFile in $ConfigFiles) {
        Write-Host "  Copied: $($ConfigFile.Name)" -ForegroundColor Green
    }
} else {
    Write-Host "  Warning: Config directory not found: $ConfigsDir" -ForegroundColor DarkYellow
}

# Summary
Write-Host ""
Write-Host "=== Deployment Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "Directory structure:" -ForegroundColor Green
Write-Host "bin/" -ForegroundColor White
Write-Host "  analyzer.exe" -ForegroundColor Gray
Write-Host "  plugins/" -ForegroundColor White
foreach ($PluginName in $PluginNames) {
    if (Test-Path (Join-Path $BinPluginsDir $PluginName)) {
        Write-Host "    $PluginName" -ForegroundColor Gray
    }
}
Write-Host "  configs/" -ForegroundColor White
if (Test-Path $BinConfigsDir) {
    $ConfigFiles = Get-ChildItem -Path $BinConfigsDir -File
    foreach ($ConfigFile in $ConfigFiles) {
        Write-Host "    $($ConfigFile.Name)" -ForegroundColor Gray
    }
}

Write-Host ""
Write-Host "To run:" -ForegroundColor Green
Write-Host "  cd bin" -ForegroundColor White
Write-Host '  .\analyzer.exe' -ForegroundColor White
Write-Host ""
