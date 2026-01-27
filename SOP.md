# 日志分析标准操作程序 (SOP)

## 目的
使用 analyzer 工具从机器人系统获取日志并进行时序分析。

## 前置条件
- 可访问目标机器人系统（默认 SSH 配置见下文）
- 配置文件：`configs/analyzer.yaml`

## 快速开始

```bash
# 自动获取并分析最新日志
./analyzer auto
```

该命令自动完成：
1. 连接远程系统
2. 查找最新 master_control 日志
3. 下载到本地
4. 自动选择插件并分析
5. 生成 CSV 和甘特图到 output 目录

## 配置准备

### 编辑配置文件

配置文件位置：`configs/analyzer.yaml`

```yaml
# 远程连接配置（按需修改）
remote:
  enabled: true
  host: "192.168.4.69"      # 机器人 IP
  port: 23                 # SSH 端口
  user: "firefly"          # SSH 用户名
  auth:
    password: "password"
  log_dir: "/home/firefly/.ros/log"  # 远程日志目录
```

**认证方式（按优先级）**：
1. SSH 密钥文件（推荐）
2. SSH Agent
3. 密码（不推荐，配置 `password` 字段）

## 常用操作

### 1. 列出远程可用日志

```bash
# 列出所有日志文件
./analyzer list-remote

# 使用模式过滤
./analyzer list-remote "master_control_*.log"
```

### 2. 分析本地日志文件

```bash
# 分析指定文件（自动选择插件）
./analyzer analyze -i logs/your.log

# 手动指定插件
./analyzer analyze -i logs/your.log --plugin master-control-analyzer

# 自定义输出目录
./analyzer analyze -i logs/your.log -o ./my_output
```

### 3. 从远程下载并分析

```bash
# 下载指定文件并分析
./analyzer analyze -i master_control_*.log --remote
```

### 4. 仅下载文件（不分析）

```bash
./analyzer download your.log
```

### 5. 查看分析结果

```bash
# 进入输出目录
cd output

# 查看 CSV 数据文件
head -20 analysis.csv

# 列出生成的甘特图
ls -lh *.png

# 使用图片查看器打开（示例）
xdg-open round_1_gantt.png   # Linux
open round_1_gantt.png       # macOS
```

## 输出说明

### CSV 文件（`output/analysis.csv`）

主要字段：
- `round_id` - 轮次 ID
- `flow_id` - 导航流程 ID
- `step_type` - 动作类型（navigation/arm/head/waist）
- `action_label` - 动作标签
- `start_rel_s` / `end_rel_s` - 相对时间（秒）
- `duration_s` - 持续时间（秒）
- `status` - 状态（ok/incomplete/pending）

### 甘特图（`output/round_*_gantt.png`）

颜色说明：
- **浅蓝色** - 导航动作
- **浅橙色** - 预打舵
- **浅绿色** - 机械臂动作
- **浅橙色** - 头部控制
- **浅紫色** - 腰部控制

## 故障排除

### SSH 连接失败
```bash
# 检查网络
ping 192.168.4.69

# 测试 SSH
ssh -p 23 firefly@192.168.4.69 "echo OK"

# 检查配置文件
./analyzer check-config
```

### 插件未找到
```bash
# 列出可用插件
./analyzer list-plugins

# 确认插件已编译
ls -lh plugins/*.so

# 重新编译插件
cargo build --package master-control-analyzer --release
```

### 分析失败
- 检查日志文件格式是否正确
- 确认磁盘空间充足
- 使用 `--verbose` 参数查看详细日志

## 高级用法

### 自定义配置文件
```bash
# 使用自定义配置
./analyzer --config my_config.yaml auto
```

### 批量分析
```bash
# 分析 logs 目录下所有日志
for log in logs/*.log; do
    ./analyzer analyze -i "$log" -o "output_$(basename $log)"
done
```
