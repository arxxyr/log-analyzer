# TUI 使用指南

## 快速开始

```bash
# 默认启动 TUI
./analyzer

# CLI 模式
./analyzer --no-tui auto
```

## 快捷键

| 按键 | 功能 |
|------|------|
| `q` / `ESC` | 退出 |
| `p` / `空格` | 暂停/恢复 |
| `↑` / `↓` | 滚动日志 |
| `PgUp` / `PgDn` | 翻页（10行） |
| `Home` / `End` | 跳转首尾 |

## 工作流阶段

1. ⏳ 初始化
2. 🔗 连接远程
3. 🔍 发现文件
4. ⬇️  下载文件（显示进度 0%-100%）
5. ⚙️  分析中
6. ✅ 完成

## 使用场景

**交互式监控（推荐）：**

```bash
./analyzer
```

**自动化脚本：**

```bash
./analyzer --no-tui auto > analysis.log 2>&1
```

**SSH 会话：**

- 终端最小 80x24
- 支持 tmux/screen

## 故障排查

**TUI 显示异常：**

```bash
export LANG=en_US.UTF-8
```

**使用 CLI 模式：**

```bash
./analyzer --no-tui auto
./analyzer analyze -i file.log
```

## 编译选项

```bash
# 包含 TUI（默认）
cargo build --release

# 精简版（无 TUI）
cargo build --release --no-default-features
```
