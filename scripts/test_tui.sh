#!/bin/bash
# TUI 功能测试脚本

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║              TUI 功能测试                                     ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# 1. 检查编译
echo "1. 检查编译状态..."
if [ ! -f "target/release/analyzer" ]; then
    echo "   ✗ 未找到 analyzer 可执行文件"
    echo "   提示：运行 cargo build --release"
    exit 1
fi
echo "   ✓ analyzer 可执行文件存在"
echo ""

# 2. 检查帮助信息
echo "2. 检查命令行参数..."
if ./target/release/analyzer --help | grep -q "no-tui"; then
    echo "   ✓ --no-tui 参数存在"
else
    echo "   ✗ --no-tui 参数不存在"
    exit 1
fi
echo ""

# 3. 测试 CLI 模式（使用 --no-tui）
echo "3. 测试 CLI 模式..."
if ./target/release/analyzer --no-tui list-plugins 2>&1 | grep -q "可用插件列表\|未找到任何可用插件"; then
    echo "   ✓ CLI 模式正常工作"
else
    echo "   ✗ CLI 模式异常"
    exit 1
fi
echo ""

# 4. 测试子命令
echo "4. 测试子命令..."
if ./target/release/analyzer check-config 2>&1 | grep -q "配置文件\|配置摘要"; then
    echo "   ✓ 子命令正常工作"
else
    echo "   ✗ 子命令异常"
    exit 1
fi
echo ""

# 5. TUI 模式（需要真实终端）
echo "5. TUI 模式测试..."
echo "   注意：TUI 需要真实终端才能运行"
echo ""
echo "   手动测试步骤："
echo "   ┌────────────────────────────────────────────────────┐"
echo "   │ $ ./target/release/analyzer                        │"
echo "   │                                                     │"
echo "   │ 预期行为：                                          │"
echo "   │ - 启动 TUI 界面                                     │"
echo "   │ - 显示标题栏、信息面板、日志窗口、状态栏            │"
echo "   │ - 按 q 或 ESC 退出                                  │"
echo "   └────────────────────────────────────────────────────┘"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ 自动化测试全部通过！"
echo ""
echo "📝 接下来："
echo "   1. 在真实终端中运行: ./target/release/analyzer"
echo "   2. 测试快捷键: q (退出), p (暂停), ↑↓ (滚动)"
echo "   3. 测试 CLI 模式: ./target/release/analyzer --no-tui auto"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
