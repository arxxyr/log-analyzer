//! Analyzer CLI - 通用日志分析器命令行工具（v0.3.0）
//!
//! 这是一个基于插件的日志分析器框架，支持：
//! - 动态加载分析器插件
//! - 远程SSH连接和文件下载
//! - 配置驱动的工作流编排
//! - 自动插件选择
//! - TUI 交互式界面（可选）

use abi_stable::{library::RootModule, std_types::RString};
use analyzer_core::*;
use analyzer_merger::{AlignmentStrategy, MergeConfig, TimelineMerger};
use analyzer_visualizer::{GanttChartGenerator, VisualizationConfig};
use analyzer_workflow::{AnalyzerConfig, WorkflowOrchestrator};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

#[cfg(feature = "tui")]
use analyzer_tui::{run_tui, App, AppState};

#[cfg(feature = "tui")]
use analyzer_remote::TransferProgress;

#[cfg(feature = "tui")]
use tokio;

// ============================================================================
// 命令行参数
// ============================================================================

/// 通用日志分析器 CLI (v0.3.0)
#[derive(Parser, Debug)]
#[command(name = "analyzer")]
#[command(about = "通用日志分析器（支持插件扩展和远程获取）", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 配置文件路径
    #[arg(short, long, default_value = "configs/analyzer.yaml", global = true)]
    config: PathBuf,

    /// 日志级别
    #[arg(long, default_value = "info", global = true)]
    log_level: String,

    /// 插件目录（覆盖配置文件）
    #[arg(long, global = true)]
    plugin_dir: Option<PathBuf>,

    /// 详细输出
    #[arg(short, long, global = true)]
    verbose: bool,

    /// 禁用 TUI 交互式界面（默认启用）
    #[arg(long, global = true)]
    no_tui: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 自动模式：获取最新日志并分析
    Auto {
        /// 指定文件模式（覆盖配置文件）
        #[arg(short, long)]
        pattern: Option<String>,

        /// 输出目录（覆盖配置文件）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 分析本地或远程文件
    Analyze {
        /// 输入文件（本地路径或远程文件名）
        #[arg(short, long)]
        input: String,

        /// 输出目录
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 手动指定插件
        #[arg(short, long)]
        plugin: Option<String>,

        /// 从远程获取
        #[arg(long)]
        remote: bool,
    },

    /// 列出远程可用的日志文件
    ListRemote {
        /// 文件模式
        pattern: Option<String>,
    },

    /// 列出所有可用的分析器插件
    ListPlugins,

    /// 下载文件（不分析）
    Download {
        /// 远程文件名
        file: String,

        /// 本地保存路径
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 验证配置文件
    CheckConfig,

    /// 多文件分析：合并多个日志源并生成统一甘特图
    Multi {
        /// 输出目录（覆盖配置文件）
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 输出文件名前缀
        #[arg(short, long, default_value = "merged")]
        prefix: String,

        /// 是否自动从远程下载
        #[arg(long)]
        auto_download: bool,
    },
}

// ============================================================================
// 插件管理
// ============================================================================

/// 插件信息
struct PluginInfo {
    name: String,
    path: PathBuf,
    metadata: PluginMetadata,
    plugin: AnalyzerPlugin_TO<'static, abi_stable::std_types::RBox<()>>,
}

/// 插件管理器
struct PluginManager {
    plugins: Vec<PluginInfo>,
}

impl PluginManager {
    fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    fn load_plugins(&mut self, plugin_dir: &Path) -> Result<()> {
        if !plugin_dir.exists() {
            warn!("插件目录不存在: {}", plugin_dir.display());
            return Ok(());
        }

        info!("扫描插件目录: {}", plugin_dir.display());

        for entry in fs::read_dir(plugin_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(ext) = path.extension() {
                #[cfg(target_os = "linux")]
                let is_lib = ext == "so";
                #[cfg(target_os = "macos")]
                let is_lib = ext == "dylib";
                #[cfg(target_os = "windows")]
                let is_lib = ext == "dll";

                if !is_lib {
                    continue;
                }

                match self.load_plugin(&path) {
                    Ok(plugin_info) => {
                        info!("  ✓ 加载插件: {}", plugin_info.name);
                        self.plugins.push(plugin_info);
                    }
                    Err(e) => {
                        warn!("  ✗ 加载失败: {} - {}", path.display(), e);
                    }
                }
            }
        }

        if self.plugins.is_empty() {
            anyhow::bail!("未找到任何可用插件");
        }

        info!("成功加载 {} 个插件", self.plugins.len());
        Ok(())
    }

    fn load_plugin(&self, path: &Path) -> Result<PluginInfo> {
        let module = AnalyzerPluginModule_Ref::load_from_file(path)
            .map_err(|e| anyhow::anyhow!("无法加载插件 {}: {:?}", path.display(), e))?;

        let plugin = module.create_plugin()();
        let metadata = plugin.metadata();
        let name = metadata.name.to_string();

        Ok(PluginInfo {
            name,
            path: path.to_path_buf(),
            metadata,
            plugin,
        })
    }

    fn find_by_name(&self, name: &str) -> Option<&PluginInfo> {
        self.plugins
            .iter()
            .find(|p| p.name == name || p.name.contains(name))
    }

    fn list_plugins(&self) {
        println!("\n可用插件列表:\n");
        for plugin in &self.plugins {
            println!("名称: {}", plugin.metadata.name);
            println!("版本: {}", plugin.metadata.version);
            println!("作者: {}", plugin.metadata.author);
            println!("描述: {}", plugin.metadata.description);
            print!("支持的文件类型: ");
            for (i, ext) in plugin.metadata.supported_extensions.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}", ext);
            }
            println!("\n路径: {}", plugin.path.display());
            println!("{}", "-".repeat(60));
        }
    }
}

// ============================================================================
// 执行分析
// ============================================================================

fn run_analysis(plugin: &PluginInfo, input_file: &Path, output_dir: &Path) -> Result<()> {
    info!("使用插件: {} v{}", plugin.metadata.name, plugin.metadata.version);
    info!("分析文件: {}", input_file.display());
    info!("输出目录: {}", output_dir.display());

    // 准备分析参数
    let analyze_args = AnalyzeArgs {
        input_file: RString::from(input_file.to_str().context("无效的路径")?),
        output_dir: RString::from(output_dir.to_str().context("无效的路径")?),
        extra_args: None.into(),
    };

    // 执行分析
    let result = plugin
        .plugin
        .analyze(analyze_args)
        .into_result()
        .context("插件分析失败")?;

    // 显示结果
    println!("\n{}", result.summary);
    println!("\n生成的文件:");
    for file in result.output_files.iter() {
        println!("  - {} ({}): {}", file.path, file.file_type, file.description);
    }

    println!("\n分析完成！");
    Ok(())
}

// ============================================================================
// TUI 支持
// ============================================================================

#[cfg(feature = "tui")]
struct TuiProgressCallback {
    state: AppState,
    file_name: String,
    last_percent: u8,
}

#[cfg(feature = "tui")]
impl TuiProgressCallback {
    fn new(state: AppState, file_name: String) -> Self {
        Self {
            state,
            file_name,
            last_percent: 0,
        }
    }
}

#[cfg(feature = "tui")]
impl TransferProgress for TuiProgressCallback {
    fn on_progress(&mut self, bytes_transferred: u64, total_bytes: u64) {
        if total_bytes == 0 {
            return;
        }
        let percent = ((bytes_transferred as f64 / total_bytes as f64) * 100.0) as u8;

        // 每25%更新一次日志
        if percent >= self.last_percent + 25 || (percent == 100 && self.last_percent < 100) {
            self.last_percent = percent;
            let mb_transferred = bytes_transferred as f64 / 1024.0 / 1024.0;
            let mb_total = total_bytes as f64 / 1024.0 / 1024.0;

            self.state.add_log(
                analyzer_tui::state::LogLevel::Info,
                format!(
                    "下载进度: {}% ({:.1}MB/{:.1}MB)",
                    percent, mb_transferred, mb_total
                ),
            );

            // 更新进度条
            self.state.set_progress(Some(
                analyzer_tui::state::ProgressInfo::new(bytes_transferred, total_bytes),
            ));
        } else {
            // 静默更新进度条
            self.state.set_progress(Some(
                analyzer_tui::state::ProgressInfo::new(bytes_transferred, total_bytes),
            ));
        }
    }

    fn on_complete(&mut self) {
        self.state.add_log(
            analyzer_tui::state::LogLevel::Info,
            format!("✓ 下载完成: {}", self.file_name),
        );
        self.state.set_progress(None);
    }

    fn on_error(&mut self, error: &str) {
        self.state.add_log(
            analyzer_tui::state::LogLevel::Error,
            format!("✗ 下载失败: {} - {}", self.file_name, error),
        );
        self.state.set_progress(None);
    }
}

#[cfg(feature = "tui")]
fn run_with_tui(config: &AnalyzerConfig) -> Result<()> {
    use analyzer_tui::state::{LogLevel, PluginDisplayInfo};

    info!("启动 TUI 模式");

    // 创建 TUI 应用状态
    let app_state = AppState::new();

    // 从配置文件构建插件显示列表（不实际加载动态库，避免重复加载）
    let mut potential_plugins = Vec::new();

    // 遍历配置文件中的分析器定义
    for analyzer in &config.analyzers {
        potential_plugins.push(PluginDisplayInfo {
            name: analyzer.plugin.clone(),
            version: "unknown".to_string(), // 版本信息将在实际加载时获取
            description: analyzer.description.clone(),
            enabled: analyzer.enabled,
            required: analyzer.required,
        });
    }

    // 设置可用插件列表
    app_state.set_available_plugins(potential_plugins);

    // 创建 TUI 应用
    let mut app = App::new(app_state.clone());

    // 使用 tokio 运行时
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // 在后台运行工作流
        let state_clone = app_state.clone();
        let mut config_clone = config.clone();

        // 在TUI模式下禁用indicatif进度条（避免破坏TUI布局）
        // TUI通过自己的日志系统显示进度
        config_clone.workflow.progress.show_progress_bar = false;

        tokio::spawn(async move {
            // 工作流循环：支持重启
            loop {
                // 执行一次工作流
                run_workflow_once(&state_clone, &config_clone).await;

                // 等待重启请求
                loop {
                    if state_clone.is_restart_requested() {
                        state_clone.clear_restart_request();
                        state_clone.add_log(LogLevel::Info, "🔄 重启工作流...");
                        state_clone.reset_for_restart();
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        break; // 跳出内循环，重新执行工作流
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        });

        // 运行 TUI
        run_tui(&mut app).await
    })?;

    Ok(())
}

/// 执行一次工作流
#[cfg(feature = "tui")]
async fn run_workflow_once(state_clone: &AppState, config_clone: &AnalyzerConfig) {
    use analyzer_tui::state::{LogLevel, WorkflowPhase};

    state_clone.add_log(LogLevel::Info, "开始工作流...");
    state_clone.set_phase(WorkflowPhase::Initializing);

    // 获取用户选择的插件列表
    let enabled_plugin_names = state_clone.enabled_plugin_names();
    state_clone.add_log(LogLevel::Info, format!("已启用 {} 个插件", enabled_plugin_names.len()));

    // 加载用户选择的插件
    state_clone.add_log(LogLevel::Info, format!("加载插件目录: {}", config_clone.local.plugin_dir.display()));
    let mut plugin_manager = PluginManager::new();
    if let Err(e) = plugin_manager.load_plugins(&config_clone.local.plugin_dir) {
        state_clone.add_log(LogLevel::Error, format!("加载插件失败: {}", e));
        state_clone.set_phase(WorkflowPhase::Error);
        return;
    }

    // 过滤只保留用户选择的插件
    plugin_manager.plugins.retain(|p| enabled_plugin_names.contains(&p.name));

    state_clone.add_log(LogLevel::Info, format!("成功加载 {} 个插件", plugin_manager.plugins.len()));

    // 在日志中列出插件
    for plugin in &plugin_manager.plugins {
        state_clone.add_log(
            LogLevel::Info,
            format!("  ✓ {} v{}", plugin.metadata.name, plugin.metadata.version)
        );
    }

    if plugin_manager.plugins.is_empty() {
        state_clone.add_log(LogLevel::Error, "未选择任何插件，无法继续");
        state_clone.set_phase(WorkflowPhase::Error);
        return;
    }

    // 清理旧输出
    if config_clone.local.cleanup_old_output && config_clone.local.output_dir.exists() {
        state_clone.add_log(LogLevel::Info, "清理旧输出文件");
        if let Ok(entries) = fs::read_dir(&config_clone.local.output_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let ext = path.extension().and_then(|e| e.to_str());
                    if matches!(ext, Some("png") | Some("csv")) {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }

    // 执行自动工作流（手动实现，使用自定义进度回调）
    use analyzer_remote::{FileTransfer, SshConfig, SshConnection};
    use analyzer_workflow::discoverer::FileDiscoverer;
    use analyzer_workflow::selector::PluginSelector;

    state_clone.set_phase(WorkflowPhase::Connecting);
    state_clone.add_log(LogLevel::Info, "连接到远程服务器...");

    // 1. 连接SSH
    let ssh_config = SshConfig {
        host: config_clone.remote.host.clone(),
        port: config_clone.remote.port,
        user: config_clone.remote.user.clone(),
        auth: config_clone.remote.auth.to_auth_method(),
        timeouts: config_clone.remote.timeouts.clone(),
    };

    let mut connection = match SshConnection::connect(ssh_config) {
        Ok(conn) => conn,
        Err(e) => {
            state_clone.add_log(LogLevel::Error, format!("SSH连接失败: {}", e));
            state_clone.set_phase(WorkflowPhase::Error);
            return;
        }
    };

    state_clone.set_phase(WorkflowPhase::Discovering);
    state_clone.add_log(LogLevel::Info, "发现日志文件...");

    // 2. 发现文件
    let discoverer = FileDiscoverer::new(config_clone.file_discovery.clone());
    let selector = PluginSelector::new(config_clone.analyzers.clone());

    let pattern = selector
        .list_enabled()
        .first()
        .map(|m| m.pattern.clone())
        .unwrap_or_else(|| "*.log".to_string());

    let file_info = match discoverer.discover_and_select_remote(
        &mut connection,
        config_clone.remote.log_dir.to_str().unwrap(),
        &pattern,
    ) {
        Ok(file) => file,
        Err(e) => {
            state_clone.add_log(LogLevel::Error, format!("文件发现失败: {}", e));
            state_clone.set_phase(WorkflowPhase::Error);
            return;
        }
    };

    state_clone.add_log(LogLevel::Info, format!("找到文件: {}", file_info.name));

    // 3. 使用自定义回调下载文件
    let local_path = config_clone.local.log_dir.join(&file_info.name);

    // 确保本地目录存在
    if let Err(e) = fs::create_dir_all(&config_clone.local.log_dir) {
        state_clone.add_log(LogLevel::Error, format!("创建目录失败: {}", e));
        state_clone.set_phase(WorkflowPhase::Error);
        return;
    }

    let mut transfer = FileTransfer::new(connection);
    let progress_callback = TuiProgressCallback::new(state_clone.clone(), file_info.name.clone());

    if let Err(e) = transfer.download_with_callback(&file_info.path, &local_path, Some(progress_callback)) {
        state_clone.add_log(LogLevel::Error, format!("下载失败: {}", e));
        state_clone.set_phase(WorkflowPhase::Error);
        return;
    }

    // 4. 选择插件
    let plugin_name = match selector.select_plugin(&file_info.name) {
        Some(mapping) => mapping.plugin.clone(),
        None => {
            state_clone.add_log(LogLevel::Error, "未找到匹配的插件");
            state_clone.set_phase(WorkflowPhase::Error);
            return;
        }
    };

    let file_to_analyze = local_path;

    // 查找插件
    let plugin = match plugin_manager.find_by_name(&plugin_name) {
        Some(p) => p,
        None => {
            state_clone.add_log(LogLevel::Error, format!("未找到插件: {}", plugin_name));
            state_clone.set_phase(WorkflowPhase::Error);
            return;
        }
    };

    state_clone.set_phase(WorkflowPhase::Analyzing);
    state_clone.add_log(
        LogLevel::Info,
        format!("开始分析: {}", file_to_analyze.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("未知文件"))
    );
    state_clone.add_log(
        LogLevel::Info,
        format!("使用插件: {} v{}", plugin.metadata.name, plugin.metadata.version)
    );

    // 执行分析
    let analyze_args = AnalyzeArgs {
        input_file: RString::from(match file_to_analyze.to_str() {
            Some(s) => s,
            None => {
                state_clone.add_log(LogLevel::Error, "无效的文件路径");
                state_clone.set_phase(WorkflowPhase::Error);
                return;
            }
        }),
        output_dir: RString::from(match config_clone.local.output_dir.to_str() {
            Some(s) => s,
            None => {
                state_clone.add_log(LogLevel::Error, "无效的输出路径");
                state_clone.set_phase(WorkflowPhase::Error);
                return;
            }
        }),
        extra_args: None.into(),
    };

    state_clone.add_log(LogLevel::Info, "解析日志中...");

    let analysis_result = match plugin.plugin.analyze(analyze_args).into_result() {
        Ok(r) => r,
        Err(e) => {
            state_clone.add_log(LogLevel::Error, format!("分析失败: {}", e));
            state_clone.set_phase(WorkflowPhase::Error);
            return;
        }
    };

    // 统计生成的文件
    let gantt_count = analysis_result.output_files.iter()
        .filter(|f| f.file_type == "png" && f.path.contains("gantt"))
        .count();
    let csv_count = analysis_result.output_files.iter()
        .filter(|f| f.file_type == "csv")
        .count();

    state_clone.add_log(
        LogLevel::Info,
        format!("生成 {} 个甘特图, {} 个CSV文件", gantt_count, csv_count)
    );

    // 显示分析结果
    state_clone.add_log(LogLevel::Info, "分析完成！");
    state_clone.add_log(LogLevel::Info, analysis_result.summary.to_string());
    state_clone.add_log(LogLevel::Info, "生成的文件:");
    for file in analysis_result.output_files.iter() {
        state_clone.add_log(
            LogLevel::Info,
            format!("  - {} ({}): {}", file.path, file.file_type, file.description)
        );
    }

    // 解析并显示 major_flow_time_summary.txt（如果存在）
    let summary_path = config_clone.local.output_dir.join("major_flow_time_summary.txt");
    if summary_path.exists() {
        match std::fs::read_to_string(&summary_path) {
            Ok(content) => {
                // 统计完整和不完整的大流程
                let mut complete_count = 0;
                let mut incomplete_count = 0;
                let mut total_time = 0.0f64;
                let mut complete_time = 0.0f64;
                let mut complete_flows = Vec::new();
                let mut incomplete_flows = Vec::new();

                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // 解析行：大流程 1 | 完整   | 1097.74秒 | 平均137.22秒/轮 | 10个轮次 | 09:29:18 -> 09:47:36
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() < 5 {
                        continue;
                    }

                    let flow_name = parts[0].trim();
                    let status = parts[1].trim();
                    let time_str = parts[2].trim();
                    let rounds = parts[4].trim();

                    // 提取时间（秒）
                    let time_seconds = if let Some(num_str) = time_str.split('秒').next() {
                        num_str.trim().parse::<f64>().unwrap_or(0.0)
                    } else {
                        0.0
                    };

                    // 提取轮次数
                    let round_count = if let Some(num_str) = rounds.split('个').next() {
                        num_str.trim().parse::<usize>().unwrap_or(0)
                    } else {
                        0
                    };

                    // 统计（注意：必须先检查"不完整"，因为它包含"完整"）
                    if status.contains("不完整") {
                        incomplete_count += 1;
                        incomplete_flows.push((flow_name.to_string(), time_seconds, round_count));
                    } else if status.contains("完整") {
                        complete_count += 1;
                        complete_time += time_seconds;
                        complete_flows.push((flow_name.to_string(), time_seconds, round_count));
                    }
                    total_time += time_seconds;
                }

                // 显示摘要
                state_clone.add_log(LogLevel::Info, "");
                state_clone.add_log(LogLevel::Info, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                state_clone.add_log(LogLevel::Info, "📊 大流程分析摘要");
                state_clone.add_log(LogLevel::Info, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                // 完整大流程
                if complete_count > 0 {
                    state_clone.add_log(
                        LogLevel::Info,
                        format!("✅ 完整大流程: {} 个 (共 {:.1} 分钟)",
                            complete_count, complete_time / 60.0)
                    );
                    // 只显示前5个
                    for (name, time, rounds) in complete_flows.iter().take(5) {
                        state_clone.add_log(
                            LogLevel::Info,
                            format!("   {} {:.1}分 {}轮",
                                name, time / 60.0, rounds)
                        );
                    }
                    if complete_flows.len() > 5 {
                        state_clone.add_log(
                            LogLevel::Info,
                            format!("   ... 还有 {} 个完整流程",
                                complete_flows.len() - 5)
                        );
                    }
                }

                // 不完整大流程
                if incomplete_count > 0 {
                    state_clone.add_log(LogLevel::Info, "");
                    state_clone.add_log(
                        LogLevel::Info,
                        format!("⚠️  不完整大流程: {} 个",
                            incomplete_count)
                    );
                    // 只显示前3个
                    for (name, time, rounds) in incomplete_flows.iter().take(3) {
                        state_clone.add_log(
                            LogLevel::Info,
                            format!("   {} {:.1}分 {}轮",
                                name, time / 60.0, rounds)
                        );
                    }
                    if incomplete_flows.len() > 3 {
                        state_clone.add_log(
                            LogLevel::Info,
                            format!("   ... 还有 {} 个不完整流程",
                                incomplete_flows.len() - 3)
                        );
                    }
                }

                state_clone.add_log(LogLevel::Info, "");
                state_clone.add_log(
                    LogLevel::Info,
                    format!("⏱️  总用时: {:.1}分 ({:.2}小时)",
                        total_time / 60.0, total_time / 3600.0)
                );
                state_clone.add_log(LogLevel::Info, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            }
            Err(e) => {
                state_clone.add_log(
                    LogLevel::Warn,
                    format!("无法读取摘要文件: {}", e)
                );
            }
        }
    }

    state_clone.add_log(LogLevel::Info, "");
    state_clone.set_phase(WorkflowPhase::Completed);
    state_clone.mark_completed();
    state_clone.add_log(LogLevel::Info, "🎉 所有任务完成！");
}

#[cfg(not(feature = "tui"))]
fn run_with_tui(_config: &AnalyzerConfig) -> Result<()> {
    anyhow::bail!("TUI 功能未启用。请使用 --features tui 编译");
}

// ============================================================================
// 主程序
// ============================================================================

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 如果没有子命令且未禁用 TUI，启动 TUI 界面
    #[cfg(feature = "tui")]
    if cli.command.is_none() && !cli.no_tui {
        // 加载配置
        let config = if cli.config.exists() {
            AnalyzerConfig::load_from_file(&cli.config)?
        } else {
            AnalyzerConfig::default()
        };

        return run_with_tui(&config);
    }

    // 初始化日志
    let log_level = if cli.verbose {
        "debug"
    } else {
        &cli.log_level
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(log_level)),
        )
        .with_target(false)
        .with_level(true)
        .init();

    debug!("配置文件路径: {:?}", cli.config);

    // 加载配置
    let config = if cli.config.exists() {
        info!("加载配置文件: {}", cli.config.display());
        AnalyzerConfig::load_from_file(&cli.config)?
    } else {
        warn!("配置文件不存在，使用默认配置");
        AnalyzerConfig::default()
    };

    // 确定插件目录
    let plugin_dir = if let Some(dir) = cli.plugin_dir {
        dir
    } else {
        config.local.plugin_dir.clone()
    };

    debug!("插件目录: {:?}", plugin_dir);

    // 加载插件
    let mut plugin_manager = PluginManager::new();
    plugin_manager.load_plugins(&plugin_dir)?;

    // 处理子命令
    match cli.command {
        Some(Commands::ListPlugins) => {
            plugin_manager.list_plugins();
            return Ok(());
        }

        Some(Commands::CheckConfig) => {
            println!("✓ 配置文件验证成功");
            println!("\n配置摘要:");
            println!("  远程连接: {}", if config.remote.enabled { "启用" } else { "禁用" });
            if config.remote.enabled {
                println!("  远程主机: {}@{}:{}", config.remote.user, config.remote.host, config.remote.port);
            }
            println!("  本地日志目录: {}", config.local.log_dir.display());
            println!("  输出目录: {}", config.local.output_dir.display());
            println!("  插件目录: {}", config.local.plugin_dir.display());
            println!("  分析器数量: {}", config.analyzers.len());
            return Ok(());
        }

        Some(Commands::ListRemote { pattern }) => {
            let mut orchestrator = WorkflowOrchestrator::new(config)?;
            let files = orchestrator.list_remote_logs(pattern.as_deref())?;

            println!("\n远程可用文件:\n");
            for file in files {
                println!("  {} ({} bytes, mtime: {})", file.name, file.size, file.mtime);
            }
            return Ok(());
        }

        Some(Commands::Download { file, output }) => {
            let mut orchestrator = WorkflowOrchestrator::new(config)?;
            let local_path = orchestrator.download_only(&file)?;

            if let Some(out) = output {
                fs::rename(&local_path, &out)?;
                println!("文件已下载到: {}", out.display());
            } else {
                println!("文件已下载到: {}", local_path.display());
            }
            return Ok(());
        }

        Some(Commands::Auto { pattern, output }) => {
            // 如果提供了覆盖参数，更新配置
            let mut config = config;
            if let Some(p) = pattern {
                if let Some(first_analyzer) = config.analyzers.first_mut() {
                    first_analyzer.pattern = p;
                }
            }
            if let Some(o) = output {
                config.local.output_dir = o;
            }

            // 清理旧输出
            if config.local.cleanup_old_output && config.local.output_dir.exists() {
                info!("清理旧输出文件");
                for entry in fs::read_dir(&config.local.output_dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str());
                        if matches!(ext, Some("png") | Some("csv")) {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
            }

            // 执行自动工作流
            let mut orchestrator = WorkflowOrchestrator::new(config.clone())?;
            let result = orchestrator.run_auto()?;

            if !result.success {
                error!("工作流执行失败");
                for err in &result.errors {
                    error!("  - {}", err);
                }
                anyhow::bail!("工作流失败");
            }

            // 获取分析文件和插件
            let file_to_analyze = result.analyzed_file.context("未找到要分析的文件")?;
            let plugin_name = result.selected_plugin.context("未选择插件")?;

            // 查找插件
            let plugin = plugin_manager
                .find_by_name(&plugin_name)
                .with_context(|| format!("未找到插件: {}", plugin_name))?;

            // 执行分析
            run_analysis(plugin, &file_to_analyze, &config.local.output_dir)?;
        }

        Some(Commands::Analyze {
            input,
            output,
            plugin: plugin_name,
            remote,
        }) => {
            let mut config = config;
            if let Some(o) = output {
                config.local.output_dir = o;
            }

            // 获取输入文件
            let input_file = if remote {
                let mut orchestrator = WorkflowOrchestrator::new(config.clone())?;
                orchestrator.download_only(&input)?
            } else {
                PathBuf::from(&input)
            };

            if !input_file.exists() {
                anyhow::bail!("文件不存在: {}", input_file.display());
            }

            // 选择插件
            let plugin = if let Some(name) = plugin_name {
                plugin_manager
                    .find_by_name(&name)
                    .with_context(|| format!("未找到插件: {}", name))?
            } else {
                // 通过 workflow selector 自动选择
                let orchestrator = WorkflowOrchestrator::new(config.clone())?;
                let file_name = input_file.file_name()
                    .and_then(|n| n.to_str())
                    .context("无效的文件名")?;

                let mapping = orchestrator
                    .selector()
                    .select_plugin(file_name)
                    .context("无法自动选择插件，请使用 --plugin 手动指定")?;

                plugin_manager
                    .find_by_name(&mapping.plugin)
                    .with_context(|| format!("未找到插件: {}", mapping.plugin))?
            };

            // 执行分析
            run_analysis(plugin, &input_file, &config.local.output_dir)?;
        }

        Some(Commands::Multi {
            output,
            prefix,
            auto_download,
        }) => {
            let mut config = config;
            if let Some(o) = output {
                config.local.output_dir = o;
            }

            // 检查多文件分析是否启用
            if !config.multi_file.enabled {
                anyhow::bail!("多文件分析未启用，请在配置文件中设置 multi_file.enabled = true");
            }

            info!("开始多文件分析");

            // 1. 准备文件列表
            let mut files_to_analyze: Vec<(PathBuf, String)> = Vec::new(); // (file_path, pattern)

            if auto_download && config.remote.enabled {
                // 从远程下载多个文件
                let mut orchestrator = WorkflowOrchestrator::new(config.clone())?;

                for pattern in &config.multi_file.auto_patterns {
                    info!("查找远程文件: {}", pattern);
                    let remote_files = orchestrator.list_remote_logs(Some(pattern))?;

                    if let Some(file_info) = remote_files.first() {
                        info!("下载: {}", file_info.name);
                        let local_path = orchestrator.download_only(&file_info.name)?;
                        files_to_analyze.push((local_path, pattern.clone()));
                    } else {
                        warn!("未找到匹配文件: {}", pattern);
                    }
                }
            } else {
                // 使用本地文件
                for pattern in &config.multi_file.auto_patterns {
                    // 在本地日志目录中查找匹配的文件
                    if let Ok(entries) = fs::read_dir(&config.local.log_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                                if glob_match::glob_match(pattern, file_name) {
                                    files_to_analyze.push((path.clone(), pattern.clone()));
                                    info!("找到本地文件: {}", path.display());
                                    break; // 只取第一个匹配的文件
                                }
                            }
                        }
                    }
                }
            }

            if files_to_analyze.is_empty() {
                anyhow::bail!("未找到任何要分析的文件");
            }

            info!("共找到 {} 个文件需要分析", files_to_analyze.len());

            // 2. 分析每个文件并收集时间线
            let mut timelines = Vec::new();

            for (file_path, pattern) in &files_to_analyze {
                info!("分析文件: {}", file_path.display());

                // 根据 pattern 找到对应的 analyzer 配置
                let analyzer_mapping = config
                    .analyzers
                    .iter()
                    .find(|a| a.pattern == *pattern)
                    .context("未找到对应的分析器配置")?;

                // 查找插件
                let plugin = plugin_manager
                    .find_by_name(&analyzer_mapping.plugin)
                    .with_context(|| format!("未找到插件: {}", analyzer_mapping.plugin))?;

                // 准备分析参数
                let analyze_args = AnalyzeArgs {
                    input_file: RString::from(file_path.to_str().context("无效的路径")?),
                    output_dir: RString::from(config.local.output_dir.to_str().context("无效的路径")?),
                    extra_args: None.into(),
                };

                // 执行分析
                let result = plugin
                    .plugin
                    .analyze(analyze_args)
                    .into_result()
                    .with_context(|| format!("分析文件失败: {}", file_path.display()))?;

                // 收集时间线
                timelines.push(result.timeline);

                info!("  ✓ 分析完成: {}", result.summary);
            }

            // 3. 合并时间线
            info!("合并时间线...");

            let merge_config = MergeConfig {
                primary_source: config.multi_file.alignment.primary_source.clone(),
                time_tolerance: config.multi_file.alignment.time_tolerance,
                alignment_strategy: match config.multi_file.alignment.strategy {
                    analyzer_workflow::config::AlignmentStrategy::Timestamp => AlignmentStrategy::Timestamp,
                    analyzer_workflow::config::AlignmentStrategy::EventBased => AlignmentStrategy::EventBased,
                },
                track_priority: config.multi_file.track_priority.clone(),
            };

            let merger = TimelineMerger::new(merge_config);
            let merged = merger.merge(timelines)?;

            info!("合并完成: {} 个事件", merged.events.len());

            // 4. 生成统一的甘特图
            let output_path = config
                .local
                .output_dir
                .join(format!("{}_gantt.png", prefix));

            info!("生成甘特图: {}", output_path.display());

            let vis_config = VisualizationConfig::default();
            let generator = GanttChartGenerator::new(vis_config);

            generator.generate_gantt_chart(
                &merged,
                output_path.to_str().context("无效的路径")?,
                &format!("多文件分析 - {}", prefix),
            )?;

            println!("\n✓ 多文件分析完成！");
            println!("  - 分析文件数: {}", files_to_analyze.len());
            println!("  - 合并事件数: {}", merged.events.len());
            println!("  - 甘特图: {}", output_path.display());
        }

        None => {
            // 默认行为：如果禁用了 TUI，执行 auto 模式
            info!("未指定命令，执行 auto 模式");

            // 清理旧输出
            if config.local.cleanup_old_output && config.local.output_dir.exists() {
                info!("清理旧输出文件");
                for entry in fs::read_dir(&config.local.output_dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str());
                        if matches!(ext, Some("png") | Some("csv")) {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
            }

            // 执行自动工作流
            let mut orchestrator = WorkflowOrchestrator::new(config.clone())?;
            let result = orchestrator.run_auto()?;

            if !result.success {
                error!("工作流执行失败");
                for err in &result.errors {
                    error!("  - {}", err);
                }
                anyhow::bail!("工作流失败");
            }

            // 获取分析文件和插件
            let file_to_analyze = result.analyzed_file.context("未找到要分析的文件")?;
            let plugin_name = result.selected_plugin.context("未选择插件")?;

            // 查找插件
            let plugin = plugin_manager
                .find_by_name(&plugin_name)
                .with_context(|| format!("未找到插件: {}", plugin_name))?;

            // 执行分析
            run_analysis(plugin, &file_to_analyze, &config.local.output_dir)?;
        }
    }

    Ok(())
}

// 简单的 glob 匹配实现
mod glob_match {
    pub fn glob_match(pattern: &str, text: &str) -> bool {
        // 简化版的 glob 匹配，支持 * 通配符
        let pattern_parts: Vec<&str> = pattern.split('*').collect();

        if pattern_parts.len() == 1 {
            // 没有通配符，直接比较
            return pattern == text;
        }

        let mut text_pos = 0;

        for (i, part) in pattern_parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if i == 0 {
                // 第一个部分必须在开头
                if !text.starts_with(part) {
                    return false;
                }
                text_pos = part.len();
            } else if i == pattern_parts.len() - 1 {
                // 最后一个部分必须在结尾
                if !text.ends_with(part) {
                    return false;
                }
            } else {
                // 中间部分
                if let Some(pos) = text[text_pos..].find(part) {
                    text_pos += pos + part.len();
                } else {
                    return false;
                }
            }
        }

        true
    }
}
