# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

这是一个 **Rust 日志分析工具**，专门用于解析和分析机器人控制系统（master_control）的日志文件。该工具能够：
- 从日志中提取导航和机械臂操作的时序信息
- 检测并分析多轮任务执行情况
- 生成 CSV 报告和甘特图可视化

## 常用命令

### 构建
```bash
# Debug 模式
cargo build

# Release 模式（推荐用于分析大型日志）
cargo build --release
```

### 运行
```bash
# 基本用法
cargo run --release -- --log <日志文件路径> --outdir <输出目录>

# 示例
cargo run --release -- --log master_control_3943506_1756803704695.log --outdir output
```

### 测试
```bash
# 运行所有测试
cargo test

# 显示详细输出
cargo test -- --nocapture
```

### 代码检查
```bash
# 格式化
cargo fmt

# Clippy 检查
cargo clippy -- -W clippy::all
```

## 架构说明

### 核心数据流
1. **日志解析** (`load_log_lines`): 读取日志文件，提取带时间戳的行
2. **轮次检测** (`detect_rounds`): 识别任务轮次边界（物体类型:0 触发新轮次）
3. **流程检测** (`detect_flows`): 解析导航和机械臂操作序列
4. **数据构建** (`build_csv_records`): 转换为结构化记录
5. **输出生成**: 
   - CSV 导出 (`export_csv`)
   - 甘特图生成 (`generate_gantt_charts`)

### 关键数据结构

- **LogLine**: 带时间戳的日志行
- **Round**: 任务轮次（包含开始/结束时间、姿态信息）
- **NavigationFlow**: 导航流程（包含导航目标和关联的各类动作）
- **ActionOperation**: 通用动作操作（支持arm/head/waist等类型，包含动作代码、标签、时间范围）
- **CsvRecord**: 输出的结构化记录

### 日志格式支持

程序支持两种日志格式：
- **旧格式**: `[timestamp] [module]: message`
- **新格式**: `[INFO] [timestamp] [module]: message`

### 日志模式匹配

程序使用正则表达式匹配特定日志模式：
- 时间戳: 支持 `[timestamp]` 和 `[INFO/WARN/ERROR/DEBUG] [timestamp]` 两种格式
- 轮次开始: `[master_control]: 物体类型: 0`（每检测到物体类型0开始新轮次）
- 姿态信息: `[master_control]: 姿态字符串: {JSON格式的姿态数据}`

### 支持的动作类型

1. **导航 (Navigation)**
   - 开始: `[navigation]: 设置目标点`
   - 目标: `设置导航目标: pos(x,y,z), ori(x,y,z,w)`
   - 完成: `[navigation]: onResultReceived. Done = 0`

2. **机械臂 (DoubleArmAction)**
   - 开始: `[DoubleArmAction]: setGoal action_type_code: <代码>`
   - 完成: `[DoubleArmAction]: <标签>: onResultReceived. Status = <状态码>`

3. **头部控制 (Head Control)**
   - 开始: `[head_control]: 设置/开始`
   - 完成: `[head_control]: 完成/结束/Done`

4. **腰部控制 (Waist Control)**
   - 开始: `[waist_control]: 设置/开始`
   - 完成: `[waist_control]: 完成/结束/Done`

### 输出文件

- `output/analysis.csv`: 包含所有操作的详细时序数据
- `output/round_N_gantt.png`: 每个轮次的甘特图（分辨率 2800px）
  - 浅蓝色: 导航动作
  - 浅绿色: 机械臂动作
  - 浅橙色: 头部控制
  - 浅紫色: 腰部控制
  - 灰色: 其他动作

## 注意事项

1. **日志编码**: 程序会自动处理 UTF-8 编码问题，使用 lossy 转换处理无效字符
2. **时间处理**: 
   - 内部使用 Unix 时间戳（秒级精度）
   - 甘特图显示北京时间（UTC+8）
   - CSV 中的时间为相对于日志开始的秒数
3. **性能**: 对于大型日志文件（>100MB），建议使用 release 模式构建
4. **依赖版本**: 使用 Rust edition 2024，主要依赖包括 plotters（图表）、csv（数据导出）、regex（模式匹配）