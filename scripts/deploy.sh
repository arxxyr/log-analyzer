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
        PLUGIN_COUNT=$((PLUGIN_COUNT + 1))
    else
        echo -e "${GRAY}警告: 未找到插件 $PLUGIN_NAME${NC}"
    fi
done
echo -e "${GREEN}成功复制 $PLUGIN_COUNT 个插件${NC}"

echo -e "\n${YELLOW}[5/5] 复制配置文件...${NC}"
if [ -d "$CONFIGS_DIR" ] && [ -r "$CONFIGS_DIR" ]; then
    CONFIG_COUNT=0
    # 复制所有文件（包括隐藏文件）
    shopt -s dotglob nullglob || true  # 包含隐藏文件，空目录不报错
    for SRC_FILE in "$CONFIGS_DIR"/*; do
        if [ -f "$SRC_FILE" ]; then
            cp "$SRC_FILE" "$BIN_CONFIGS_DIR/"
            echo -e "${GREEN}已复制: $(basename "$SRC_FILE")${NC}"
            CONFIG_COUNT=$((CONFIG_COUNT + 1))
        fi
    done
    shopt -u dotglob nullglob || true

    if [ "$CONFIG_COUNT" -eq 0 ]; then
        echo -e "${GRAY}警告: 配置目录为空 $CONFIGS_DIR${NC}"
    else
        echo -e "${GREEN}成功复制 $CONFIG_COUNT 个配置文件${NC}"
    fi
else
    echo -e "${RED}错误: 无法访问配置目录 $CONFIGS_DIR${NC}"
    echo -e "${YELLOW}请检查目录是否存在及权限设置${NC}"
fi

echo -e "\n${CYAN}=== 部署完成 ===${NC}"
echo -e "\n${GRAY}提示: 字体已嵌入到 libmaster_control_analyzer.so 中（32MB）${NC}"
echo -e "${GRAY}      首次运行时会自动提取到 bin/fonts/ 目录${NC}"
echo -e "${GRAY}      字体目录会随程序一起打包，无需额外安装${NC}"
echo -e "\n${GREEN}目录结构:${NC}"
echo "bin/"
echo "├── $EXE_NAME"
echo "├── fonts/ (首次运行后自动生成)"
echo "├── plugins/"
PLUGIN_FOUND_COUNT=0
for PLUGIN_NAME in "${PLUGIN_NAMES[@]}"; do
    if [ -f "$BIN_PLUGINS_DIR/$PLUGIN_NAME" ]; then
        PLUGIN_FOUND_COUNT=$((PLUGIN_FOUND_COUNT + 1))
    fi
done
PLUGIN_CURRENT=0
for PLUGIN_NAME in "${PLUGIN_NAMES[@]}"; do
    if [ -f "$BIN_PLUGINS_DIR/$PLUGIN_NAME" ]; then
        PLUGIN_CURRENT=$((PLUGIN_CURRENT + 1))
        if [ "$PLUGIN_CURRENT" -lt "$PLUGIN_FOUND_COUNT" ]; then
            echo "│   ├── $PLUGIN_NAME"
        else
            echo "│   └── $PLUGIN_NAME"
        fi
    fi
done
echo "└── configs/"
if [ -d "$BIN_CONFIGS_DIR" ]; then
    CONFIG_FILES=("$BIN_CONFIGS_DIR"/*)
    for CONFIG_FILE in "${CONFIG_FILES[@]}"; do
        if [ -f "$CONFIG_FILE" ]; then
            echo "    └── $(basename "$CONFIG_FILE")"
        fi
    done
fi

echo -e "\n${GREEN}运行方式:${NC}"
echo "  cd bin"
echo "  ./analyzer"
echo ""
