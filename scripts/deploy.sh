#!/usr/bin/env bash
# Bash 部署脚本 - 将编译产物收集到 bin 目录
# 用法: ./scripts/deploy.sh [release|debug]

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
GRAY='\033[0;37m'
NC='\033[0m' # No Color

# 默认编译配置
PROFILE="${1:-release}"

echo -e "${CYAN}=== 日志分析器部署脚本 ===${NC}"
echo -e "${GREEN}编译配置: $PROFILE${NC}"

# 项目根目录
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# 目标目录
BIN_DIR="$ROOT_DIR/bin"
BIN_PLUGINS_DIR="$BIN_DIR/plugins"
BIN_CONFIGS_DIR="$BIN_DIR/configs"

# 源目录
TARGET_DIR="$ROOT_DIR/target/$PROFILE"
CONFIGS_DIR="$ROOT_DIR/configs"

echo -e "\n${YELLOW}[1/5] 清理旧的 bin 目录...${NC}"
if [ -d "$BIN_DIR" ]; then
    rm -rf "$BIN_DIR"
    echo -e "${GRAY}已删除旧目录: $BIN_DIR${NC}"
fi

echo -e "\n${YELLOW}[2/5] 创建目录结构...${NC}"
mkdir -p "$BIN_DIR"
mkdir -p "$BIN_PLUGINS_DIR"
mkdir -p "$BIN_CONFIGS_DIR"
echo -e "${GRAY}已创建: bin/${NC}"
echo -e "${GRAY}已创建: bin/plugins/${NC}"
echo -e "${GRAY}已创建: bin/configs/${NC}"

echo -e "\n${YELLOW}[3/5] 复制可执行文件...${NC}"
EXE_NAME="analyzer"
EXE_PATH="$TARGET_DIR/$EXE_NAME"
if [ -f "$EXE_PATH" ]; then
    cp "$EXE_PATH" "$BIN_DIR/"
    echo -e "${GREEN}已复制: $EXE_NAME${NC}"
else
    echo -e "${RED}错误: 找不到可执行文件 $EXE_PATH${NC}"
    echo -e "${YELLOW}请先运行: cargo build --release${NC}"
    exit 1
fi

echo -e "\n${YELLOW}[4/5] 复制插件...${NC}"
PLUGIN_COUNT=0

# 根据操作系统确定插件扩展名
if [[ "$OSTYPE" == "darwin"* ]]; then
    PLUGIN_EXT="dylib"
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    PLUGIN_EXT="dll"
else
    PLUGIN_EXT="so"
fi

# 插件列表
PLUGIN_NAMES=(
    "libmaster_control_analyzer.$PLUGIN_EXT"
    "libcpp_demo_analyzer.$PLUGIN_EXT"
)

for PLUGIN_NAME in "${PLUGIN_NAMES[@]}"; do
    PLUGIN_PATH="$TARGET_DIR/$PLUGIN_NAME"
    if [ -f "$PLUGIN_PATH" ]; then
        cp "$PLUGIN_PATH" "$BIN_PLUGINS_DIR/"
        echo -e "${GREEN}已复制: $PLUGIN_NAME${NC}"
        ((PLUGIN_COUNT++))
    else
        echo -e "${GRAY}警告: 未找到插件 $PLUGIN_NAME${NC}"
    fi
done
echo -e "${GREEN}成功复制 $PLUGIN_COUNT 个插件${NC}"

echo -e "\n${YELLOW}[5/5] 复制配置文件...${NC}"
if [ -d "$CONFIGS_DIR" ]; then
    cp -r "$CONFIGS_DIR"/* "$BIN_CONFIGS_DIR/"
    for CONFIG_FILE in "$BIN_CONFIGS_DIR"/*; do
        if [ -f "$CONFIG_FILE" ]; then
            echo -e "${GREEN}已复制: $(basename "$CONFIG_FILE")${NC}"
        fi
    done
else
    echo -e "${GRAY}警告: 未找到配置目录 $CONFIGS_DIR${NC}"
fi

echo -e "\n${CYAN}=== 部署完成 ===${NC}"
echo -e "\n${GREEN}目录结构:${NC}"
echo "bin/"
echo "├── $EXE_NAME"
echo "├── plugins/"
for PLUGIN_NAME in "${PLUGIN_NAMES[@]}"; do
    if [ -f "$BIN_PLUGINS_DIR/$PLUGIN_NAME" ]; then
        echo "│   └── $PLUGIN_NAME"
    fi
done
echo "└── configs/"
if [ -d "$BIN_CONFIGS_DIR" ]; then
    CONFIG_FILES=("$BIN_CONFIGS_DIR"/*)
    LAST_INDEX=$((${#CONFIG_FILES[@]} - 1))
    for i in "${!CONFIG_FILES[@]}"; do
        CONFIG_FILE="${CONFIG_FILES[$i]}"
        if [ -f "$CONFIG_FILE" ]; then
            if [ "$i" -eq "$LAST_INDEX" ]; then
                echo "    └── $(basename "$CONFIG_FILE")"
            else
                echo "    ├── $(basename "$CONFIG_FILE")"
            fi
        fi
    done
fi

echo -e "\n${GREEN}运行方式:${NC}"
echo "  cd bin"
echo "  ./analyzer"
echo ""
