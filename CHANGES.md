# 变更日志

本文件记录项目的所有重要变更。

版本规范：[语义化版本 2.0.0](https://semver.org/lang/zh-CN/)

---

## v0.5.7 (2026-04-23) - 当前版本

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
