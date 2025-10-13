# 🚀 Analyzer v0.3.0 快速开始

## 一行命令开始使用

```bash
cd 111 && ./analyzer auto
```

## 📋 5 分钟上手指南

### 1️⃣ 查看帮助和插件

```bash
./analyzer --help
./analyzer list-plugins
```

### 2️⃣ 配置远程连接（可选）

编辑 `configs/analyzer.yaml`：

```yaml
remote:
  enabled: true
  host: "你的远程主机"
  user: "用户名"
  auth:
    use_agent: true
```

### 3️⃣ 验证配置

```bash
./analyzer check-config
```

### 4️⃣ 分析日志

```bash
# 自动模式（推荐）
./analyzer auto

# 指定本地文件
./analyzer analyze -i logs/your.log

# 从远程获取并分析
./analyzer analyze -i your.log --remote
```

## 📚 常用命令速查

| 命令 | 用途 |
|------|------|
| `./analyzer auto` | 🔥 自动获取最新日志并分析 |
| `./analyzer list-remote` | 📋 列出远程可用日志 |
| `./analyzer download FILE` | ⬇️ 下载文件（不分析） |
| `./analyzer analyze -i FILE` | 🔍 分析本地文件 |
| `./analyzer list-plugins` | 🔌 列出所有插件 |
| `./analyzer check-config` | ✅ 验证配置文件 |

## 🎯 典型使用场景

### 场景 A：日常自动化分析

```bash
# 每日运行这一条命令即可
./analyzer auto
```

### 场景 B：查看远程有哪些日志

```bash
./analyzer list-remote
```

### 场景 C：分析特定的历史日志

```bash
# 如果在本地
./analyzer analyze -i logs/master_control_20241013.log

# 如果在远程
./analyzer analyze -i master_control_20241013.log --remote
```

### 场景 D：离线使用（先下载再分析）

```bash
# 1. 下载
./analyzer download master_control_123.log

# 2. 分析
./analyzer analyze -i logs/master_control_123.log
```

## 🔧 配置文件关键部分

```yaml
# configs/analyzer.yaml

# 🌐 远程连接
remote:
  enabled: true
  host: "192.168.4.69"
  port: 23
  user: "firefly"

# 📂 本地路径
local:
  log_dir: "./logs"
  output_dir: "./output"

# 🔌 插件映射（核心功能）
analyzers:
  - name: "master-control"
    pattern: "master_control_*.log"  # 匹配模式
    plugin: "master-control-analyzer" # 插件名称
    enabled: true
```

## 📊 输出文件说明

分析后在 `output/` 生成：

- 📈 `analysis.csv` - 详细时序数据
- 📊 `major_flow_stats.csv` - 流程统计
- 🎨 `round_XX_gantt.png` - 甘特图（每轮）
- 📝 `*.txt` - 统计摘要

## 🆚 与旧版本对比

```bash
# 旧版本（analyze.sh）
./analyze.sh

# 新版本（analyzer v0.3.0）
./analyzer auto
```

**优势：**
- ✅ 更快（Rust 原生）
- ✅ 更安全（类型检查）
- ✅ 跨平台（Windows 支持）
- ✅ 可配置（YAML 配置文件）
- ✅ 自动插件选择

## ⚡ 性能提示

```bash
# 详细输出（调试用）
./analyzer --verbose auto

# 静默模式（只看结果）
./analyzer --log-level error auto

# 自定义输出目录
./analyzer auto -o /path/to/output
```

## 🐛 遇到问题？

### 检查清单

1. ✅ 配置文件是否正确？ `./analyzer check-config`
2. ✅ 插件是否加载？ `./analyzer list-plugins`
3. ✅ SSH 能否连接？ `ssh user@host -p port`
4. ✅ 文件是否存在？ `ls -lh logs/`

### 常见错误

**错误：无法连接远程**
```bash
# 解决：检查 SSH 配置
./analyzer check-config
ssh firefly@192.168.4.69 -p 23
```

**错误：找不到插件**
```bash
# 解决：检查插件目录
ls -lh plugins/
./analyzer --plugin-dir ./plugins list-plugins
```

**错误：没有找到文件**
```bash
# 解决：列出可用文件
./analyzer list-remote
```

## 📖 更多文档

- 📘 **完整使用说明** → `111/README.md`
- 🏗️ **架构设计文档** → `docs/WORKFLOW_ARCHITECTURE.md`
- 🔌 **插件开发指南** → `docs/PLUGIN_ARCHITECTURE.md`
- 🔄 **迁移指南** → `docs/MIGRATION_GUIDE.md`

## 💡 专业技巧

### 技巧 1：批量分析

```bash
# 列出所有远程日志
./analyzer list-remote > files.txt

# 逐个分析
cat files.txt | while read file; do
    ./analyzer analyze -i "$file" --remote -o "output_${file%.*}"
done
```

### 技巧 2：自动化脚本

```bash
#!/bin/bash
# auto-analyze.sh

cd /path/to/analyzer
./analyzer auto

# 可选：上传结果到其他位置
# rsync -av output/ user@server:/backup/
```

### 技巧 3：使用环境变量

```bash
# 临时覆盖配置
export ANALYZER_CONFIG=my_config.yaml
./analyzer auto
```

## 🎉 完成！

现在你已经掌握了 analyzer v0.3.0 的基本用法。

需要帮助？查看完整文档或提交 Issue。

---

**版本：** v0.3.0
**更新日期：** 2025-10-13
