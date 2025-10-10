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

### 模块结构

项目采用清晰的模块化设计，主要模块如下：

1. **models.rs** - 核心数据结构定义
   - `Args`: 命令行参数
   - `LogLine`: 日志行（时间戳 + 内容）
   - `Round`: 任务轮次
   - `SubStep`: 子步骤
   - `ActionOperation`: 动作操作（arm/head/waist）
   - `NavigationFlow`: 导航流程
   - `MajorFlow`: 大流程（多轮次组成）
   - `CsvRecord`: CSV 导出记录

2. **utils.rs** - 工具函数
   - `timestamp_to_beijing_time()`: Unix 时间戳转北京时间字符串

3. **parser.rs** - 日志解析
   - `load_log_lines()`: 从日志文件加载并解析时间戳行

4. **round_detector.rs** - 轮次与大流程检测
   - `detect_rounds()`: 基于循环标记检测任务轮次
   - `detect_major_flows()`: 检测大流程（完整/不完整）
   - `ts_to_round_id()`: 时间戳映射到轮次 ID

5. **flow_detector.rs** - 导航流程与动作检测
   - `detect_flows()`: 检测导航流程及关联动作
   - 辅助函数：`create_action`, `add_action_to_flow`, `finish_action` 等

6. **csv_exporter.rs** - CSV 数据导出
   - `build_csv_records()`: 构建 CSV 记录
   - `export_csv()`: 导出主分析 CSV
   - `export_major_flow_stats()`: 导出大流程统计
   - `generate_action_timeline_csv()`: 生成动作时间轴

7. **gantt.rs** - 甘特图可视化
   - `generate_gantt_charts()`: 为每个轮次生成甘特图
   - 辅助函数：`draw_sub_steps`, `draw_time_label`, `draw_main_label`

8. **lib.rs** - 库入口
   - 模块声明与公共 API 导出
   - 提供统一的库接口

9. **main.rs** - 程序入口（115 行）
   - 参数解析
   - 调用各模块功能
   - 输出分析结果

### 核心数据流
1. **日志解析** (`parser::load_log_lines`): 读取日志文件，提取带时间戳的行
2. **轮次检测** (`round_detector::detect_rounds`): 识别任务轮次边界
3. **大流程检测** (`round_detector::detect_major_flows`): 分析完整/不完整流程
4. **流程检测** (`flow_detector::detect_flows`): 解析导航和机械臂操作序列
5. **数据构建** (`csv_exporter::build_csv_records`): 转换为结构化记录
6. **输出生成**:
   - CSV 导出 (`csv_exporter::export_csv` 等)
   - 甘特图生成 (`gantt::generate_gantt_charts`)

### 关键数据结构

- **LogLine**: 带时间戳的日志行
- **Round**: 任务轮次（基于循环标记检测，包含开始/结束时间、姿态信息）
- **NavigationFlow**: 导航流程（包含导航目标和关联的各类动作）
- **ActionOperation**: 通用动作操作（支持arm/head/waist等类型，包含动作名称、代码、时间范围）
- **CsvRecord**: 输出的结构化记录

### 日志格式

程序使用新格式：`[INFO/WARN/ERROR/DEBUG] [timestamp] [module]: message`

### 日志模式匹配

程序使用正则表达式匹配特定日志模式：
- 时间戳: `[INFO/WARN/ERROR/DEBUG] [timestamp]` 格式
- 轮次开始: `[发布日志节点]: [INFO] loop: 开始循环`
- 轮次结束: `[发布日志节点]: [INFO] loop: 结束当前循环`
- 姿态信息: `[master_control]: 姿态字符串: {JSON格式的姿态数据}`

### 支持的动作类型

1. **导航 (Navigation)**
   - 开始: `[导航]: NavAction2[NavAction2] - 开始执行`
   - 目标: `设置导航目标: pos(x,y,z), ori(x,y,z,w)`
   - 完成: `[导航]: NavAction2[NavAction2] - 执行完成，结果:`

2. **机械臂 (DoubleArmAction)**
   - 开始: `[机械臂]: DoubleArmAction[<动作名称>] - 开始执行`
   - 动作代码设置: `[机械臂]: DoubleArmAction setGoal action_type_code: <代码>`
   - 结果回调: `[机械臂]: [RESULT CALLBACK] - 机械臂动作完成，状态: <状态码>`
   - 执行完成: `[机械臂]: DoubleArmAction[<动作名称>] - 执行完成，结果: <结果>`

3. **头部控制 (Head Control)**
   - 开始: `[头部控制]: HeadControlAction2[head_control] - 开始执行`
   - 完成: `[头部控制]: HeadControlAction2[head_control] - 执行完成`

4. **腰部控制 (Waist Control)**
   - 开始: `[腰部]: WaistAction2[WaistAction2] - 开始执行`
   - 完成: `[腰部]: WaistAction2[WaistAction2] - 执行完成，结果:`

### 输出文件

- `output/analysis.csv`: 包含所有操作的详细时序数据
- `output/round_N_gantt.png`: 每个轮次的甘特图（分辨率 2800px）
  - 浅蓝色: 导航动作
  - 浅绿色: 机械臂动作
  - 浅橙色: 头部控制
  - 浅紫色: 腰部控制
  - 灰色: 其他动作

## 开发指南

### 添加新的动作类型

若需支持新的动作类型（如 `gripper`、`vision` 等）：

1. 在 `flow_detector.rs` 中添加正则表达式模式
2. 在 `detect_flows()` 中添加匹配逻辑
3. 使用 `create_action()` 创建新动作
4. 在 `gantt.rs` 的颜色映射中添加新颜色
5. 更新 CLAUDE.md 的"支持的动作类型"部分

### 添加新的输出格式

若需导出其他格式（如 JSON、Excel）：

1. 在 `csv_exporter.rs` 中添加新的导出函数
2. 或创建新模块（如 `json_exporter.rs`）
3. 在 `lib.rs` 中声明模块
4. 在 `main.rs` 中调用导出函数

### 修改日志解析规则

若日志格式发生变化：

1. 更新 `parser.rs` 中的时间戳正则表达式
2. 更新 `round_detector.rs` 中的循环标记匹配
3. 更新 `flow_detector.rs` 中的动作日志模式
4. 运行测试确保向后兼容性

### 代码风格

遵循全局 CLAUDE.md 中的 Rust 编码规范：
- 使用 `snake_case` 命名函数和变量
- 使用 `UpperCamelCase` 命名类型
- 优先使用 `&str`/`&[T]` 而非 `String`/`Vec<T>` 作为参数
- 使用 `?` 操作符处理错误
- 添加中文文档注释说明模块和函数用途
- 运行 `cargo fmt` 和 `cargo clippy` 保持代码质量

## 注意事项

1. **日志编码**: 程序会自动处理 UTF-8 编码问题，使用 lossy 转换处理无效字符
2. **时间处理**:
   - 内部使用 Unix 时间戳（秒级精度）
   - 甘特图显示北京时间（UTC+8）
   - CSV 中的时间为相对于日志开始的秒数
3. **轮次检测**: 基于循环标记（`loop: 开始循环` 和 `loop: 结束当前循环`）自动检测任务轮次
4. **性能**: 对于大型日志文件（>100MB），建议使用 release 模式构建
5. **依赖版本**: 使用 Rust edition 2024，主要依赖包括 plotters（图表）、csv（数据导出）、regex（模式匹配）
6. **模块化设计**: 各模块职责清晰，修改时尽量保持单一职责原则（SRP），避免跨模块耦合