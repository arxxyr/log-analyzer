#!/bin/bash

# Master Control 日志分析脚本
# 用途：自动从远程机器人系统下载日志并进行分析

# 配置参数
REMOTE_HOST="192.168.4.69"
REMOTE_PORT="23"
REMOTE_USER="firefly"
REMOTE_LOG_DIR="/home/firefly/.ros/log"
LOCAL_OUTPUT_DIR="output"
ANALYZER_BIN="./analyzer"  # 新的插件化分析器

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印函数
print_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
print_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
print_error() { echo -e "${RED}[ERROR]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }

# 显示帮助信息
show_help() {
    echo "使用方法:"
    echo "  $0                    # 自动获取并分析最新日志"
    echo "  $0 <log_file>        # 分析指定的日志文件（本地或远程）"
    echo "  $0 -h                # 显示此帮助信息"
    echo "  $0 --list            # 列出所有可用的分析器插件"
    echo ""
    echo "示例:"
    echo "  $0                                          # 自动模式"
    echo "  $0 master_control_3463_1755747167392.log   # 指定文件"
    echo "  $0 --list                                   # 列出插件"
    echo ""
    echo "配置:"
    echo "  远程主机: $REMOTE_USER@$REMOTE_HOST:$REMOTE_PORT"
    echo "  日志路径: $REMOTE_LOG_DIR"
    echo "  输出目录: $LOCAL_OUTPUT_DIR"
    echo "  分析器:   $ANALYZER_BIN (插件化架构)"
    echo ""
    echo "注意:"
    echo "  本脚本使用插件化的分析器架构 (v0.2.0+)"
    echo "  插件会自动根据文件扩展名选择合适的分析器"
}

# 检查分析程序
check_analyzer() {
    if [ ! -f "$ANALYZER_BIN" ]; then
        print_error "分析程序未找到: $ANALYZER_BIN"
        exit 1
    fi

    # 添加执行权限
    if [ ! -x "$ANALYZER_BIN" ]; then
        print_warning "添加执行权限..."
        chmod +x "$ANALYZER_BIN"
    fi

    # 检查插件目录
    local plugin_dir="./plugins"
    if [ ! -d "$plugin_dir" ]; then
        print_warning "插件目录未找到: $plugin_dir"
        print_info "正在创建插件目录..."
        mkdir -p "$plugin_dir"
    fi

    # 检查 master-control-analyzer 插件
    local plugin_file="$plugin_dir/libmaster_control_analyzer.so"
    if [ ! -f "$plugin_file" ]; then
        print_warning "master-control-analyzer 插件未找到"
        print_error "无法找到插件文件，请确保已构建插件"
        exit 1
    fi
}

# 获取远程最新日志
get_latest_remote_log() {
    # 先显示提示信息（输出到stderr，不会混入返回值）
    print_info "连接到远程系统..." >&2
    
    LATEST_LOG=$(ssh -p $REMOTE_PORT $REMOTE_USER@$REMOTE_HOST \
        "cd $REMOTE_LOG_DIR && ls -t master_control_*.log 2>/dev/null | head -1")
    
    if [ -z "$LATEST_LOG" ]; then
        print_error "远程系统没有找到master_control日志文件" >&2
        exit 1
    fi
    
    echo "$LATEST_LOG"
}

# 下载日志文件
download_log() {
    local log_file=$1
    local remote_path="$REMOTE_LOG_DIR/$log_file"

    print_info "下载日志: $log_file"

    # 确保 logs 目录存在
    mkdir -p ./logs

    # 检查本地是否已存在
    if [ -f "./logs/$log_file" ]; then
        print_warning "本地已存在文件 ./logs/$log_file，将覆盖"
    fi

    # 下载
    scp -P $REMOTE_PORT $REMOTE_USER@$REMOTE_HOST:$remote_path ./logs/$log_file

    if [ $? -eq 0 ]; then
        print_success "下载完成 ($(ls -lh ./logs/$log_file | awk '{print $5}'))"
        return 0
    else
        return 1
    fi
}

# 分析日志
analyze_log() {
    local log_file=$1

    print_info "开始分析: $log_file"

    # 创建输出目录
    mkdir -p $LOCAL_OUTPUT_DIR

    # 清理旧的输出
    rm -f $LOCAL_OUTPUT_DIR/*.png $LOCAL_OUTPUT_DIR/*.csv 2>/dev/null

    # 运行分析（使用新的命令行参数格式）
    $ANALYZER_BIN -i "$log_file" -o "$LOCAL_OUTPUT_DIR"
    
    if [ $? -eq 0 ]; then
        print_success "分析完成"
        
        # 统计结果
        local csv_count=$(ls -1 $LOCAL_OUTPUT_DIR/*.csv 2>/dev/null | wc -l)
        local png_count=$(ls -1 $LOCAL_OUTPUT_DIR/*.png 2>/dev/null | wc -l)
        
        echo ""
        print_info "生成结果:"
        echo "  CSV文件: $csv_count 个"
        echo "  甘特图: $png_count 个"
        echo "  输出目录: $(pwd)/$LOCAL_OUTPUT_DIR"
        
        # 显示部分甘特图列表
        if [ $png_count -gt 0 ]; then
            echo ""
            print_info "甘特图列表 (前5个):"
            ls -lh $LOCAL_OUTPUT_DIR/*.png | head -5 | awk '{print "  " $9 " (" $5 ")"}'
            if [ $png_count -gt 5 ]; then
                echo "  ... 共 $png_count 个文件"
            fi
        fi
        
        return 0
    else
        print_error "分析失败"
        return 1
    fi
}

# 主程序
main() {
    echo "======================================"
    echo "   Master Control 日志分析工具"
    echo "======================================"
    echo ""
    
    # 检查分析程序
    check_analyzer
    
    # 处理参数
    if [ "$1" == "-h" ] || [ "$1" == "--help" ]; then
        show_help
        exit 0
    fi

    if [ "$1" == "--list" ]; then
        print_info "列出所有可用的分析器插件:"
        echo ""
        $ANALYZER_BIN --list
        exit 0
    fi
    
    if [ $# -eq 0 ]; then
        # 自动模式：获取最新日志
        print_info "自动模式：获取最新日志"
        LOG_FILE=$(get_latest_remote_log)

        # 清理LOG_FILE中可能的颜色代码
        LOG_FILE=$(echo "$LOG_FILE" | sed 's/\x1B\[[0-9;]*m//g')

        print_success "找到最新日志: $LOG_FILE"

        if download_log "$LOG_FILE"; then
            analyze_log "./logs/$LOG_FILE"
        else
            print_error "下载失败"
            exit 1
        fi

    else
        # 指定文件模式
        LOG_FILE=$1

        # 先检查 logs 目录下是否存在
        if [ -f "./logs/$LOG_FILE" ]; then
            # logs 目录下的文件存在，直接分析
            print_info "分析 logs 目录下的文件: $LOG_FILE"
            analyze_log "./logs/$LOG_FILE"

        elif [ -f "$LOG_FILE" ]; then
            # 当前目录下文件存在，直接分析
            print_info "分析本地文件: $LOG_FILE"
            analyze_log "$LOG_FILE"

        else
            # 尝试从远程下载
            print_info "本地未找到文件，尝试从远程下载: $LOG_FILE"

            if download_log "$LOG_FILE"; then
                analyze_log "./logs/$LOG_FILE"
            else
                print_error "文件 $LOG_FILE 不存在（本地和远程都未找到）"
                echo ""
                show_help
                exit 1
            fi
        fi
    fi
    
    echo ""
    print_success "全部完成！"
}

# 捕获错误
set -e
trap 'if [ $? -ne 0 ]; then print_error "执行出错"; fi' EXIT

# 运行主程序
main "$@"