# 变更日志

本文件记录项目的所有重要变更。

版本规范：[语义化版本 2.0.0](https://semver.org/lang/zh-CN/)

---

## v0.5.8 (2026-09-01) - 当前版本

### 适配新版 master_control 日志格式（循环标记）

- **LogNode 模板注册行被当作真实事件**：新版日志的部分模板正文不再含
  `{variable}` 占位符（如 `message='[收尾循环] 最终循环完成'`），与真实事件
  文本一字不差，靠内容正则无法区分。新增 `is_log_node_registration()`，在
  轮次检测与流程检测入口按 `LogNode[xxx] - 初始化完成:` 整行跳过
- **初始循环结束标记变更**：新增 `[初始循环] 初始循环结束，扫码成功=<bool>,
  气密位状态=N`；要求布尔值为具体 `true`/`false`，双重避开模板占位符
- **收尾循环结束标记变更**：新增 `[收尾循环] 最终循环完成`（旧版为
  `[收尾循环] 双工位收尾完成`），与前述注册行过滤配套生效

### 修复暂停时间被重复扣除导致轮次时长归零

- **问题现象**：一次真实暂停会被多种模式同时命中——失败暂停同时产生
  `TaskGraphExecutor: 节点 ... 失败` 与 `ROS2ActionAdapter[...] - 暂停`，
  用户暂停同时产生 `PauseTaskNode` 与 `TaskGraphExecutor: 用户请求暂停任务`。
  `total_pause_duration()` 朴素逐事件求和，同一段暂停被扣两次，
  轮次有效时长被扣成 0（实测 172738 日志轮次 7：应为 69.5s，显示 0.0s；
  224083 日志有 2 个轮次归零）
- **修复**：新增 `merge_intervals()` / `merged_pause_duration()`，
  先合并重叠区间再累加；`Round` 与 `ActionOperation` 的暂停统计、
  甘特图的 `pause_time_before()` 全部改用合并结果

### 修复甘特图跨暂停动作冲出时间轴

- **问题现象**：横轴按扣除暂停后的压缩坐标绘制，但动作长度仍按墙钟计算。
  暂停期间挂起的 BT 节点条长达 323s 而轮次有效时长仅 69.5s，
  长条冲出坐标轴，其余动作被挤成无法辨认的细线
- **修复**：新增 `compressed_duration()`，起止两端用同一套压缩映射，
  长度取两端压缩坐标之差；横轴上限由墙钟总时长改为有效时长

### macOS 甘特图字体不可用（本地分析无法出图）

- **问题现象**：macOS 上所有甘特图生成失败，报
  `Font loading error: FontUnavailable`，只能得到 CSV
- **根因**：plotters 的 ab_glyph 后端只在自身注册表里按名字查字体，
  既不查 fontconfig 也不查 CoreText。Linux 分支已用 `fc-match` +
  `register_font` 注册字体文件，而 macOS 分支只返回字体名从未注册
- **修复**：macOS 按候选路径查找系统 CJK 字体文件并注册（PingFang 在
  macOS 15+ 位于带哈希的 `AssetsV2` 目录，需动态查找；另备
  Hiragino Sans GB / STHeiti / Songti / Arial Unicode）。
  字体校验失败提示按平台区分，不再对 macOS 提示安装 Linux 字体包

### 软件自身日志时区改为 UTC+8

- `tracing` 默认输出 UTC 时间戳，与日志内容中的北京时间对不上。
  新增 `Utc8Time` 计时器固定 UTC+8，格式
  `2026-09-01 21:15:22.869321 +08:00`，不受运行机器时区影响

---

## v0.5.7 (2026-04-23)

### 适配新版 master_control 日志格式（视觉/手臂/BT 节点）

- **DetObjPose 视觉时间错误**：新版日志改为 `DetObjPose start[multi] ...` /
  `DetObjPose multi done right=... left=... count=N`，旧正则
  `DetObjPose done goal_pose=` 完全匹配不上，导致视觉子动作 `end_ts` 永远为
  `None`、甘特图与 CSV 上耗时显示为异常值。新正则
  `DetObjPose(?:\s+\w+)?\s+done\b` 同时兼容 single / multi 等所有变体
- **arm_move 失败被吞 + 状态硬编码**：新版日志同时存在
  `arm_move response: result code=N msg=...`（回调线程）和
  `arm_move response: success cmd=N` / `failure cmd=N code=X message=...`
  （主线程）。原正则不认 `failure` 分支，且 `result code=N` 时无视 N 的值
  一律设 `status=ok`，失败被静默标记成成功。新正则三选一捕获，并按真实
  捕获组判定 `ok` / `failed_<code>`
- **BehaviorTree 节点失败标记不识别**：日志真实存在
  `========== BehaviorTree 节点失败 ==========`（与 `节点结束` 并列），
  旧正则只认前者，导致失败节点的 `BtNodeContext` 永远不关闭，其
  sub_actions 会泄漏并附加到下一个 BT 节点。新正则
  `BehaviorTree\s*节点(?:结束|失败)` 同时识别两种结束 marker
- **跳过节点未扣除等待时间**：失败暂停 (`节点 X 失败，进入失败暂停状态`)
  对端只匹配 `重试节点`，但用户经常通过
  `跳过节点 X（视为成功）` 直接放行，新版日志即出现一次 7m39s 的等待未被
  扣除。`RETRY_RESUME_REGEX` 扩为 `(?:重试节点|跳过节点)`

---

## v0.3.3 (2026-01-27)

### CI/CD 优化

- **构建缓存**：使用 Swatinem/rust-cache 替代 sccache（更稳定）
- **平台支持**：移除 macOS x64 构建，只保留 ARM64
- **语义化版本**：
  - Release: `v0.3.3+abc123`（只有 commit hash）
  - Dev: `v0.3.3+20260127.abc123`（日期 + commit hash）

### 文档更新

- 更新 SOP.md，移除 TUI 相关内容
- 更新 README.md 和 CLAUDE.md，反映当前版本状态

---

## v0.3.2 (2026-01-27)

### 功能修复

- **暂停时间扣除**：修复 cycle_duration_stats.csv 和 cycle_duration_stats.png 中暂停时间未扣除的问题
- **新增暂停检测模式**：支持 `TaskGraphExecutor: 用户请求暂停任务` 模式
- **甘特图中文显示**：修复中文字符不显示问题

### 性能优化

- **mimalloc**：使用 mimalloc 作为全局内存分配器

### 代码优化

- 多处 clippy 警告修复
- 代码简化与重构

---

## v0.3.1 (2026-01-26)

### 功能增强

- 字体目录整理：删除冗余的 `assests` 目录，统一使用 `fonts/`
- Git 仓库瘦身：使用 git filter-repo 清理历史中的大文件
- 部署脚本更新：修正字体路径

---

## v0.3.0 (2025-10)

### 重大变更

- **插件架构**：从单体应用重构为插件架构
- **远程连接**：新增 SSH/SCP 支持（analyzer-remote）
- **工作流编排**：配置驱动的自动化流程（analyzer-workflow）
- **TUI 界面**：可选的交互式终端界面（analyzer-tui，使用 `--tui` 启用）
- **时间线系统**：多日志源合并和统一可视化

### 新增模块

- `analyzer-core` - 插件接口定义
- `analyzer-cli` - CLI 主程序
- `analyzer-remote` - 远程连接模块
- `analyzer-workflow` - 工作流编排模块
- `analyzer-tui` - TUI 界面模块
- `analyzer-merger` - 时间线合并模块
- `analyzer-visualizer` - 可视化模块

### CLI 变化

```bash
# 默认行为：自动模式
./analyzer

# 子命令
./analyzer auto           # 自动获取并分析
./analyzer analyze        # 分析指定文件
./analyzer list-remote    # 列出远程文件
./analyzer download       # 下载文件
./analyzer list-plugins   # 列出插件
./analyzer check-config   # 验证配置
./analyzer multi          # 多文件分析

# TUI 模式（可选）
./analyzer --tui
```

---

## v0.2.0 (2025-09)

### 重大变更

- **架构重构**：从单体应用改为插件架构
- **CLI 变化**：新的命令行接口
- **构建方式**：使用 workspace 管理多个包
- **插件系统**：支持动态加载多个分析器

---

## v0.1.0 (2025-08)

### 初始版本

- 基础日志分析功能
- 轮次检测
- 导航流程分析
- CSV 导出
- 甘特图生成
