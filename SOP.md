# Master Control 日志分析标准操作程序 (SOP)

## 目的
本SOP描述了如何从机器人系统获取master_control日志并进行时序分析的标准流程。

## 前置条件
- 已获得编译好的master_control_analyzer二进制文件
- 拥有目标机器人系统的SSH访问权限
- 目标系统IP: 192.168.5.9
- SSH端口: 23
- 用户名: firefly
- 确保二进制文件在当前目录: `./master_control_analyzer`

## 快速开始

最简单的使用方式（推荐）：
```bash
# 自动获取并分析最新日志
./analyze.sh
```

该命令会自动：
1. 连接到远程系统
2. 找到最新的master_control日志
3. 下载到本地
4. 运行分析
5. 生成结果在output目录

## 详细操作步骤

### 1. 查找最新的日志文件

```bash
ssh -p 23 firefly@192.168.5.9 "cd /home/firefly/.ros/log && pwd && ls -lrt | grep master"
```

**预期输出**：
- 显示当前目录路径 `/home/firefly/.ros/log`
- 列出所有master_control日志文件，按时间排序
- 最新的文件在列表底部

**注意事项**：
- 记录最新日志文件名，格式类似：`master_control_3463_1755747167392.log`

### 2. 下载日志文件到本地

```bash
scp -P 23 firefly@192.168.5.9:/home/firefly/.ros/log/<日志文件名> ./
```

**示例**：
```bash
scp -P 23 firefly@192.168.5.9:/home/firefly/.ros/log/master_control_3463_1755747167392.log ./
```

**预期结果**：
- 日志文件下载到当前目录
- 显示传输进度和速度

### 3. 运行分析程序

#### 方法一：使用一键脚本（推荐）
```bash
# 自动获取并分析最新日志
./analyze.sh

# 分析指定文件
./analyze.sh master_control_3463_1755747167392.log
```

#### 方法二：手动运行分析程序
```bash
./master_control_analyzer --log <日志文件名>
```

**示例**：
```bash
./master_control_analyzer --log master_control_3463_1755747167392.log
```

**预期输出**：
```
Detected XX rounds
Detected XXX navigation flows
Actions: XXX nav, XXX arm, XXX head, XXX waist
Generated XXXX CSV records
CSV exported to output/analysis.csv
Gantt chart saved: output/round_X_gantt.png
...
Analysis complete! Output in: output
```

### 4. 查看分析结果

#### 4.1 CSV数据文件
```bash
# 查看CSV文件前10行
head -10 output/analysis.csv

# 使用Excel或其他表格软件打开
# 文件位置: output/analysis.csv
```

#### 4.2 甘特图
```bash
# 列出所有生成的甘特图
ls -lh output/*.png

# 使用图片查看器打开
# 文件位置: output/round_*_gantt.png
```

## 输出说明

### CSV文件字段
- `round_id`: 轮次ID
- `flow_id`: 导航流程ID
- `step_type`: 动作类型 (navigation/arm/head/waist)
- `action_label`: 动作标签
- `start_rel_s`: 相对开始时间（秒）
- `end_rel_s`: 相对结束时间（秒）
- `duration_s`: 持续时间（秒）
- `status`: 状态 (ok/incomplete/pending)

### 甘特图说明
- **浅蓝色**: 导航动作
- **浅绿色**: 机械臂动作
- **浅橙色**: 头部控制
- **浅紫色**: 腰部控制
- **时间格式**: `>X.Xs<` 表示动作耗时

## 故障排除

### 问题1：SSH连接失败
**解决方案**：
- 检查网络连接：`ping 192.168.5.9`
- 确认SSH服务运行中
- 确认端口23开放

### 问题2：日志文件不存在
**解决方案**：
- 确认路径正确：`/home/firefly/.ros/log/`
- 检查是否有读取权限
- 确认master_control节点已运行并生成日志

### 问题3：分析程序报错"找不到程序"
**解决方案**：
- 确认master_control_analyzer二进制文件在当前目录
- 检查文件权限：`ls -l master_control_analyzer`
- 添加执行权限：`chmod +x master_control_analyzer`

### 问题4：分析程序运行报错
**解决方案**：
- 确认日志格式正确
- 检查是否有足够的磁盘空间
- 确认output目录有写入权限

## 批量处理

如需分析多个日志文件：
```bash
# 下载多个文件
for file in file1.log file2.log file3.log; do
    scp -P 23 firefly@192.168.5.9:/home/firefly/.ros/log/$file ./
done

# 批量分析
for file in *.log; do
    ./master_control_analyzer --log $file --outdir output_$file
done
```

## 准备工作

### 获取二进制文件
如果没有二进制文件，需要先编译：
```bash
# 在有Rust环境的机器上编译
cargo build --release
cp target/release/master_control_analyzer ./
```

或直接使用提供的编译好的二进制文件：
```bash
# 确保文件有执行权限
chmod +x master_control_analyzer
```

## 注意事项

1. **二进制文件**：确保master_control_analyzer在当前目录且有执行权限
2. **日志文件大小**：大型日志文件（>100MB）可能需要更多处理时间
3. **时区**：程序输出的北京时间（UTC+8）
4. **内存使用**：处理超大日志文件时注意系统内存
5. **输出目录**：默认为`output/`，可通过`--outdir`参数修改

## 相关文档
- [README.md](README.md) - 项目说明和快速开始
- [CLAUDE.md](CLAUDE.md) - 代码架构说明
- [analyze.sh](analyze.sh) - 一键分析脚本