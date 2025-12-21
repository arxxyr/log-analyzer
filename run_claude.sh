#!/usr/bin/env bash
set -euo pipefail

# 用法：
#   ./run-claude-with-proxy.sh            # 默认 http://127.0.0.1:10808
#   ./run-claude-with-proxy.sh http://127.0.0.1:7890
#   ./run-claude-with-proxy.sh socks5://127.0.0.1:10808

proxy_default="http://127.0.0.1:10808"
proxy="${1:-$proxy_default}"

# 给当前进程及其子进程设置代理
export HTTP_PROXY="$proxy"
export HTTPS_PROXY="$proxy"
export ALL_PROXY="$proxy"
export NO_PROXY="localhost,127.0.0.1,::1,.local"

echo "Using proxy: $proxy"
echo "HTTP_PROXY  = $HTTP_PROXY"
echo "HTTPS_PROXY = $HTTPS_PROXY"
echo "ALL_PROXY   = $ALL_PROXY"
echo "NO_PROXY    = $NO_PROXY"

exec claude --dangerously-skip-permissions
