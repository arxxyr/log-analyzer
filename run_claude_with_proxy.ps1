#!/usr/bin/env pwsh
#requires -Version 7.0

param(
    # 你自己的代理地址，支持 http / socks5
    [string]$Proxy = 'http://127.0.0.1:10808',

    # NO_PROXY 列表
    [string]$NoProxy = 'localhost,127.0.0.1,::1,.local',

    # 是否同步设置 WinHTTP 代理
    [switch]$SetWinHttp,

    # claude 可执行文件路径（为空则自动找）
    [string]$ClaudePath = '',

    # 传给 claude 的参数
    [string[]]$ClaudeArgs = @('--dangerously-skip-permissions'),

    # 不启动 claude，只打印环境变量
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-KV([string]$k, [string]$v) {
    Write-Host ("{0,-10} = {1}" -f $k, $v)
}

function Resolve-ClaudePath([string]$hint) {
    if ($hint -and (Test-Path -LiteralPath $hint)) {
        return (Resolve-Path -LiteralPath $hint).Path
    }
    $cmd = Get-Command 'claude' -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $default = Join-Path $env:USERPROFILE '.local\bin\claude.exe'
    if (Test-Path $default) { return (Resolve-Path $default).Path }
    throw '找不到 claude，可用 -ClaudePath 指定路径。'
}

# 设置代理环境变量
$env:HTTP_PROXY  = $Proxy
$env:HTTPS_PROXY = $Proxy
$env:ALL_PROXY   = $Proxy
$env:NO_PROXY    = $NoProxy

Write-Host "已设置当前进程代理："
Write-KV "HTTP_PROXY"  $env:HTTP_PROXY
Write-KV "HTTPS_PROXY" $env:HTTPS_PROXY
Write-KV "ALL_PROXY"   $env:ALL_PROXY
Write-KV "NO_PROXY"    $env:NO_PROXY
Write-Host ''

# 设置 WinHTTP 代理（仅 http/https）
if ($SetWinHttp) {
    if ($Proxy -match '^http') {
        $proxyHostPort = ($Proxy -replace '^https?://', '')
        Write-Host "设置 WinHTTP 代理: $proxyHostPort"
        & netsh winhttp set proxy $proxyHostPort "bypass-list=$NoProxy"
    } else {
        Write-Host "WinHTTP 不支持 socks5，已跳过。" -ForegroundColor Yellow
    }
    Write-Host ''
}

if ($DryRun) {
    Write-Host "DryRun：不启动 claude。"
    exit 0
}

$claudeExe = Resolve-ClaudePath $ClaudePath
Write-Host "claude 路径：$claudeExe"
Write-Host "启动参数：$($ClaudeArgs -join ' ')"
Write-Host ''

& $claudeExe @ClaudeArgs
