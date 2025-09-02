# Master Control Analyzer

机器人控制系统日志时序分析工具，用于解析和可视化master_control节点的执行流程。

## 功能特性

- 🔍 **自动检测任务轮次**：识别物体类型触发的任务边界
- 📊 **多维度时序分析**：导航、机械臂、头部、腰部动作的时间统计
- 📈 **甘特图可视化**：自动生成每个轮次的执行时序图
- 📝 **CSV数据导出**：结构化输出便于进一步分析
- ⏱️ **精确时间显示**：支持毫秒级时间精度，使用`>time<`格式标注

## 快速开始

### 安装

```bash
# 克隆项目
git clone <repository_url>
cd master_control_analyzer

# 编译（需要Rust环境）
cargo build --release
```

### 基本使用

```bash
# 分析本地日志文件
./target/release/master_control_analyzer --log <日志文件路径>

# 指定输出目录
./target/release/master_control_analyzer --log <日志文件路径> --outdir <输出目录>
```

### 一键分析（从远程获取并分析）

```bash
# 自动获取最新日志并分析
./analyze.sh

# 分析指定的日志文件（本地或远程）
./analyze.sh master_control_3463_1755747167392.log

# 查看帮助
./analyze.sh -h
```

## 系统要求

- Rust 1.70+ (推荐使用nightly)
- Linux/macOS/Windows
- 至少2GB可用内存（处理大型日志时）

## 输出说明

### 目录结构
```
output/
├── analysis.csv          # 时序数据表
├── round_1_gantt.png     # 第1轮次甘特图
├── round_2_gantt.png     # 第2轮次甘特图
└── ...
```

### CSV字段说明

| 字段 | 说明 | 示例 |
|------|------|------|
| round_id | 轮次ID | 1, 2, 3... |
| flow_id | 导航流程ID | 1, 2, 3... |
| step_type | 动作类型 | navigation, arm, head, waist |
| action_code | 动作代码 | 1001, 2001... |
| action_label | 动作标签 | "right get", "导航", "头部控制" |
| start_rel_s | 相对开始时间(秒) | 10.5 |
| end_rel_s | 相对结束时间(秒) | 15.3 |
| duration_s | 持续时间(秒) | 4.8 |
| status | 执行状态 | ok, incomplete, pending |

### 甘特图颜色说明

- 🔵 **浅蓝色**：导航动作
- 🟢 **浅绿色**：机械臂动作  
- 🟠 **浅橙色**：头部控制
- 🟣 **浅紫色**：腰部控制

## 支持的日志格式

### 新格式（ROS2）
```
[INFO] [1756803961.109211072] [master_control]: 物体类型: 0
[INFO] [1756803961.109228863] [导航]: NavAction2[NavAction2] - 开始执行
[INFO] [1756803961.109397739] [DoubleArmAction]: DoubleArmAction setGoal action_type_code: 1
```

### 旧格式
```
[1756803961.109211072] [master_control]: 物体类型: 0
[1756803961.109228863] [cmd_navigation_action]: NavAction: 设置目标点
```

## 高级用法

### 批量处理
```bash
# 分析多个日志文件
for file in *.log; do
    ./target/release/master_control_analyzer --log $file --outdir output_$file
done
```

### 过滤特定轮次
```bash
# 只查看特定轮次的数据
grep "^10," output/analysis.csv > round_10_data.csv
```

### 统计分析
```bash
# 统计各类动作数量
cut -d',' -f6 output/analysis.csv | sort | uniq -c

# 计算平均执行时间
awk -F',' '$6=="arm" {sum+=$15; count++} END {print sum/count}' output/analysis.csv
```

## 常见问题

### Q: 程序报错"No timestamped lines found"
A: 检查日志文件格式是否正确，确保包含时间戳信息。

### Q: 甘特图中的动作重叠
A: 这是正常的，表示多个动作并行执行（如导航时同时控制头部和腰部）。

### Q: 如何处理超大日志文件（>500MB）
A: 
1. 确保使用Release模式编译：`cargo build --release`
2. 增加系统内存或使用分页处理
3. 考虑按时间段分割日志文件

## 开发

### 项目结构
```
master_control_analyzer/
├── src/
│   └── main.rs           # 主程序逻辑
├── Cargo.toml            # 项目配置
├── CLAUDE.md            # 代码架构说明
├── SOP.md               # 操作流程文档
├── README.md            # 本文档
└── analyze.sh           # 一键分析脚本
```

### 构建选项
```bash
# Debug模式（开发用）
cargo build

# Release模式（生产用）
cargo build --release

# 运行测试
cargo test

# 代码检查
cargo clippy -- -W clippy::all
```

## 贡献

欢迎提交Issue和Pull Request。提交代码前请确保：
- 通过所有测试
- 代码风格符合Rust规范
- 更新相关文档

## 许可证

MIT License

## 联系方式

如有问题或建议，请提交Issue或联系维护者。

## 更新日志

### v0.1.0 (2024-09)
- 初始版本发布
- 支持导航、机械臂、头部、腰部动作检测
- 自动生成甘特图
- CSV数据导出功能