# PowerShell 部署脚本 - 将编译产物收集到 bin 目录
# 用法: .\scripts\deploy-windows.ps1 [release|debug]

param(
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

Write-Host "=== 日志分析器部署脚本 ===" -ForegroundColor Cyan
Write-Host "编译配置: $Profile" -ForegroundColor Green

# 项目根目录
$RootDir = Split-Path -Parent $PSScriptRoot

# 从 Cargo.toml 提取版本号
$CargoToml = Get-Content (Join-Path $RootDir "Cargo.toml") -Raw
if ($CargoToml -match '\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Host "错误: 无法从 Cargo.toml 提取版本号" -ForegroundColor Red
    exit 1
}
Write-Host "版本号: v$Version" -ForegroundColor Green

# 目录定义
$BinDir = Join-Path $RootDir "bin"
$BinPluginsDir = Join-Path $BinDir "plugins"
$BinConfigsDir = Join-Path $BinDir "configs"
$BinFontsDir = Join-Path $BinDir "fonts"
$TargetDir = Join-Path $RootDir "target\$Profile"
$ConfigsDir = Join-Path $RootDir "configs"
$FontsDir = Join-Path $RootDir "assests\fonts"

# Step 1: 清理旧的 bin 目录
Write-Host ""
Write-Host "[1/7] 清理旧的 bin 目录..." -ForegroundColor Yellow
if (Test-Path $BinDir) {
    Remove-Item -Recurse -Force $BinDir
    Write-Host "  已删除旧目录: $BinDir" -ForegroundColor Gray
}

# Step 2: 创建目录结构
Write-Host ""
Write-Host "[2/7] 创建目录结构..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $BinPluginsDir | Out-Null
New-Item -ItemType Directory -Force -Path $BinConfigsDir | Out-Null
New-Item -ItemType Directory -Force -Path $BinFontsDir | Out-Null
Write-Host "  已创建: bin/" -ForegroundColor Gray
Write-Host "  已创建: bin/plugins/" -ForegroundColor Gray
Write-Host "  已创建: bin/configs/" -ForegroundColor Gray
Write-Host "  已创建: bin/fonts/" -ForegroundColor Gray

# Step 3: 复制可执行文件
Write-Host ""
Write-Host "[3/7] 复制可执行文件..." -ForegroundColor Yellow
$ExeName = "analyzer.exe"
$ExePath = Join-Path $TargetDir $ExeName
if (Test-Path $ExePath) {
    Copy-Item $ExePath -Destination $BinDir
    Write-Host "  已复制: $ExeName" -ForegroundColor Green
} else {
    Write-Host "  错误: 找不到可执行文件 $ExePath" -ForegroundColor Red
    Write-Host "  请先运行: cargo build --release" -ForegroundColor Yellow
    exit 1
}

# Step 4: 复制插件
Write-Host ""
Write-Host "[4/7] 复制插件..." -ForegroundColor Yellow
$PluginNames = @(
    "master_control_analyzer.dll"
)
$PluginCount = 0
foreach ($PluginName in $PluginNames) {
    $PluginPath = Join-Path $TargetDir $PluginName
    if (Test-Path $PluginPath) {
        Copy-Item $PluginPath -Destination $BinPluginsDir
        Write-Host "  已复制: $PluginName" -ForegroundColor Green
        $PluginCount++
    } else {
        Write-Host "  警告: 未找到插件 $PluginName" -ForegroundColor DarkYellow
    }
}
Write-Host "  成功复制 $PluginCount 个插件" -ForegroundColor Green

# Step 5: 复制配置文件
Write-Host ""
Write-Host "[5/7] 复制配置文件..." -ForegroundColor Yellow
if (Test-Path $ConfigsDir) {
    $ConfigFiles = Get-ChildItem -Path $ConfigsDir -File
    $ConfigCount = 0
    foreach ($ConfigFile in $ConfigFiles) {
        Copy-Item $ConfigFile.FullName -Destination $BinConfigsDir
        Write-Host "  已复制: $($ConfigFile.Name)" -ForegroundColor Green
        $ConfigCount++
    }
    if ($ConfigCount -eq 0) {
        Write-Host "  警告: 配置目录为空 $ConfigsDir" -ForegroundColor DarkYellow
    } else {
        Write-Host "  成功复制 $ConfigCount 个配置文件" -ForegroundColor Green
    }
} else {
    Write-Host "  错误: 无法访问配置目录 $ConfigsDir" -ForegroundColor Red
}

# Step 6: 复制字体文件
Write-Host ""
Write-Host "[6/7] 复制字体文件..." -ForegroundColor Yellow
if (Test-Path $FontsDir) {
    $FontFile = Join-Path $FontsDir "SarasaTermSCNerd-Regular.ttf"
    if (Test-Path $FontFile) {
        Copy-Item $FontFile -Destination $BinFontsDir
        Write-Host "  已复制: SarasaTermSCNerd-Regular.ttf" -ForegroundColor Green
    } else {
        Write-Host "  警告: 未找到字体文件" -ForegroundColor DarkYellow
    }
} else {
    Write-Host "  警告: 字体目录不存在 $FontsDir" -ForegroundColor DarkYellow
}

# 部署完成提示
Write-Host ""
Write-Host "=== 部署完成 ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "目录结构:" -ForegroundColor Green
Write-Host "bin/"
Write-Host "├── $ExeName"
Write-Host "├── fonts/"
Write-Host "│   └── SarasaTermSCNerd-Regular.ttf"
Write-Host "├── plugins/"

# 显示插件列表
$FoundPlugins = @()
foreach ($PluginName in $PluginNames) {
    if (Test-Path (Join-Path $BinPluginsDir $PluginName)) {
        $FoundPlugins += $PluginName
    }
}
for ($i = 0; $i -lt $FoundPlugins.Count; $i++) {
    if ($i -lt $FoundPlugins.Count - 1) {
        Write-Host "│   ├── $($FoundPlugins[$i])"
    } else {
        Write-Host "│   └── $($FoundPlugins[$i])"
    }
}

Write-Host "└── configs/"
if (Test-Path $BinConfigsDir) {
    $ConfigFiles = Get-ChildItem -Path $BinConfigsDir -File
    foreach ($ConfigFile in $ConfigFiles) {
        Write-Host "    └── $($ConfigFile.Name)"
    }
}

# Step 7: 创建版本压缩包
Write-Host ""
Write-Host "[7/7] 创建版本压缩包..." -ForegroundColor Yellow
$ZipName = "analyzer-v$Version.zip"
$ZipPath = Join-Path $BinDir $ZipName

# 删除旧的 zip 文件
Get-ChildItem -Path $BinDir -Filter "analyzer-v*.zip" | Remove-Item -Force

# 创建新的 zip 文件
Push-Location $BinDir
try {
    # 获取所有要压缩的文件（排除 zip 文件）
    $ItemsToCompress = Get-ChildItem -Path $BinDir -Exclude "*.zip"
    Compress-Archive -Path $ItemsToCompress.FullName -DestinationPath $ZipPath -Force
    Write-Host "  已创建: bin/$ZipName" -ForegroundColor Green
} finally {
    Pop-Location
}

Write-Host ""
Write-Host "运行方式:" -ForegroundColor Green
Write-Host "  cd bin"
Write-Host '  .\analyzer.exe'
Write-Host ""
Write-Host "分发方式:" -ForegroundColor Green
Write-Host "  将 bin/$ZipName 发送给用户"
Write-Host ""
