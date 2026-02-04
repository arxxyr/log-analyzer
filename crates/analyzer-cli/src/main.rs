//! Analyzer CLI - 通用日志分析器命令行工具
//!
//! 这是一个基于插件的日志分析器框架，支持：
//! - 动态加载分析器插件
//! - 远程SSH连接和文件下载
//! - 配置驱动的工作流编排
//! - 自动插件选择

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use abi_stable::std_types::RString;
use analyzer_core::timeline::Track;
use analyzer_core::*;
use analyzer_merger::{AlignmentStrategy, MergeConfig, TimelineMerger};
use analyzer_visualizer::{GanttChartGenerator, VisualizationConfig};
use analyzer_workflow::{AnalysisTask, AnalyzerConfig, WorkflowOrchestrator};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

// ============================================================================
// 责任链模式：轮次时间段传递
// ============================================================================

/// 轮次时间范围
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoundTimeRange {
    round_id: usize,
    start: f64,
    end: f64,
}

/// 传递给后续分析器的上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnalyzerContext {
    /// 轮次时间范围列表
    round_time_ranges: Vec<RoundTimeRange>,
}

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

        // 收集所有插件库路径
        let mut lib_paths: Vec<PathBuf> = Vec::new();
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

                if is_lib {
                    lib_paths.push(path);
                }
            }
        }

        // 排序确保加载顺序一致
        lib_paths.sort();

        // 依次加载每个插件
        // 注意：abi_stable 有静态变量问题，需要在加载每个库后立即获取 metadata
        for path in lib_paths {
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

        if self.plugins.is_empty() {
            anyhow::bail!("未找到任何可用插件");
        }

        info!("成功加载 {} 个插件", self.plugins.len());
        Ok(())
    }

    fn load_plugin(&self, path: &Path) -> Result<PluginInfo> {
        use abi_stable::library::lib_header_from_path;

        // 使用 abi_stable 的 lib_header_from_path 加载插件
        // 这会正确处理 LibHeader 结构并获取根模块
        let lib_header = lib_header_from_path(path)
            .map_err(|e| anyhow::anyhow!("无法加载插件库 {}: {:?}", path.display(), e))?;

        // 初始化根模块并检查 ABI 兼容性
        let module_ref = lib_header
            .init_root_module::<AnalyzerPluginModule_Ref>()
            .map_err(|e| anyhow::anyhow!("初始化插件模块失败 {}: {:?}", path.display(), e))?;

        // 立即创建插件实例并获取元数据
        let plugin = module_ref.create_plugin()();
        let metadata = plugin.metadata();
        let name = metadata.name.to_string();

        debug!("插件 {} 元数据: name={}", path.display(), name);

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
// 可视化配置
// ============================================================================

/// 从配置文件创建可视化配置（动态加载泳道优先级和颜色）
fn create_visualization_config(config: &AnalyzerConfig) -> VisualizationConfig {
    let mut vis_config = VisualizationConfig::default();

    // 从配置文件加载 track_priority
    if !config.multi_file.track_priority.is_empty() {
        vis_config.track_priority = config.multi_file.track_priority.clone();
    }

    // 根据配置的 track_priority 动态添加默认颜色
    // 如果泳道没有配置颜色，使用默认颜色
    let default_colors = [
        "#E8F4F8", // 浅蓝灰 - RoundMarker
        "#ADD8E6", // 浅蓝 - Navigation
        "#90EE90", // 浅绿 - Arm
        "#FFB366", // 浅橙 - Head
        "#DDA0DD", // 浅紫 - Waist
        "#FFE4B5", // 小麦色
        "#B0E0E6", // 粉蓝
        "#F0E68C", // 卡其
    ];

    for (track_name, _priority) in &config.multi_file.track_priority {
        if !vis_config.track_colors.contains_key(track_name) {
            // 根据优先级分配颜色
            let priority = *_priority as usize;
            let color = default_colors.get(priority).unwrap_or(&"#CCCCCC");
            vis_config
                .track_colors
                .insert(track_name.clone(), color.to_string());
        }
    }

    vis_config
}

// ============================================================================
// 执行分析
// ============================================================================

/// 执行分析（基础版本）
fn run_analysis(
    plugin: &PluginInfo,
    input_file: &Path,
    output_dir: &Path,
) -> Result<AnalyzeResult> {
    run_analysis_with_context(plugin, input_file, output_dir, None)
}

/// 执行分析（带上下文）
fn run_analysis_with_context(
    plugin: &PluginInfo,
    input_file: &Path,
    output_dir: &Path,
    context: Option<&AnalyzerContext>,
) -> Result<AnalyzeResult> {
    info!(
        "使用插件: {} v{}",
        plugin.metadata.name, plugin.metadata.version
    );
    info!("分析文件: {}", input_file.display());
    info!("输出目录: {}", output_dir.display());

    // 准备分析参数
    let extra_args = context
        .and_then(|ctx| serde_json::to_string(ctx).ok())
        .map(RString::from);

    let analyze_args = AnalyzeArgs {
        input_file: RString::from(input_file.to_str().context("无效的路径")?),
        output_dir: RString::from(output_dir.to_str().context("无效的路径")?),
        extra_args: extra_args.into(),
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
        println!(
            "  - {} ({}): {}",
            file.path, file.file_type, file.description
        );
    }

    println!("\n分析完成！");
    Ok(result)
}

/// 从 Timeline 提取轮次时间范围
fn extract_round_time_ranges(timeline: &timeline::Timeline) -> Vec<RoundTimeRange> {
    use abi_stable::std_types::ROption;

    let mut ranges = Vec::new();
    let mut round_id = 0;

    for event in timeline.events.iter() {
        if event.track == Track::RoundMarker
            && let ROption::RSome(end) = &event.end_time
        {
            round_id += 1;
            ranges.push(RoundTimeRange {
                round_id,
                start: event.start_time,
                end: *end,
            });
        }
    }

    ranges
}

/// 运行多文件分析（责任链模式）
fn run_multi_file_analysis(
    tasks: &[AnalysisTask],
    plugin_manager: &PluginManager,
    config: &AnalyzerConfig,
) -> Result<Vec<AnalyzeResult>> {
    let mut results = Vec::new();
    let mut context: Option<AnalyzerContext> = None;

    // 按主时间轴优先排序：先分析 is_primary=true 的任务
    let mut sorted_tasks: Vec<_> = tasks.to_vec();
    sorted_tasks.sort_by_key(|t| std::cmp::Reverse(t.is_primary));

    for task in &sorted_tasks {
        info!(
            "分析文件: {} (插件: {}, 主时间轴: {})",
            task.file_name, task.plugin_name, task.is_primary
        );

        let plugin = plugin_manager
            .find_by_name(&task.plugin_name)
            .with_context(|| format!("未找到插件: {}", task.plugin_name))?;

        // 执行分析（主时间轴不需要上下文，后续分析器需要）
        let result = if task.is_primary {
            run_analysis(plugin, &task.local_file, &config.local.output_dir)?
        } else {
            run_analysis_with_context(
                plugin,
                &task.local_file,
                &config.local.output_dir,
                context.as_ref(),
            )?
        };

        // 如果是主时间轴，提取轮次时间范围作为上下文
        if task.is_primary {
            let ranges = extract_round_time_ranges(&result.timeline);
            info!("提取到 {} 个轮次时间范围", ranges.len());
            context = Some(AnalyzerContext {
                round_time_ranges: ranges,
            });
        }

        results.push(result);
    }

    Ok(results)
}

/// 合并多个 Timeline 并生成统一甘特图
fn merge_and_visualize(
    results: &[AnalyzeResult],
    config: &AnalyzerConfig,
    output_prefix: &str,
) -> Result<()> {
    if results.len() <= 1 {
        info!("只有一个分析结果，无需合并");
        return Ok(());
    }

    info!("合并 {} 个时间线...", results.len());

    // 提取所有时间线
    let timelines: Vec<_> = results.iter().map(|r| r.timeline.clone()).collect();

    // 创建合并配置
    let merge_config = MergeConfig {
        primary_source: config.multi_file.alignment.primary_source.clone(),
        time_tolerance: config.multi_file.alignment.time_tolerance,
        alignment_strategy: match config.multi_file.alignment.strategy {
            analyzer_workflow::config::AlignmentStrategy::Timestamp => AlignmentStrategy::Timestamp,
            analyzer_workflow::config::AlignmentStrategy::EventBased => {
                AlignmentStrategy::EventBased
            }
        },
        track_priority: config.multi_file.track_priority.clone(),
    };

    // 合并时间线
    let merger = TimelineMerger::new(merge_config);
    let merged = merger.merge(timelines)?;

    info!("合并完成: {} 个事件", merged.events.len());

    // 生成统一甘特图
    let output_path = config
        .local
        .output_dir
        .join(format!("{}_merged_gantt.png", output_prefix));

    info!("生成统一甘特图: {}", output_path.display());

    // 从配置文件动态创建可视化配置
    let vis_config = create_visualization_config(config);
    let generator = GanttChartGenerator::new(vis_config);

    generator.generate_gantt_chart(
        &merged,
        output_path.to_str().context("无效的路径")?,
        "多源合并甘特图",
    )?;

    println!("\n✓ 多文件分析完成！");
    println!("  - 分析文件数: {}", results.len());
    println!("  - 合并事件数: {}", merged.events.len());
    println!("  - 统一甘特图: {}", output_path.display());

    Ok(())
}

// ============================================================================
// 主程序
// ============================================================================

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 初始化日志
    let log_level = if cli.verbose { "debug" } else { &cli.log_level };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level)),
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
            println!(
                "  远程连接: {}",
                if config.remote.enabled {
                    "启用"
                } else {
                    "禁用"
                }
            );
            if config.remote.enabled {
                println!(
                    "  远程主机: {}@{}:{}",
                    config.remote.user, config.remote.host, config.remote.port
                );
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
                println!(
                    "  {} ({} bytes, mtime: {})",
                    file.name, file.size, file.mtime
                );
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
            if let Some(p) = pattern
                && let Some(first_analyzer) = config.analyzers.first_mut()
            {
                first_analyzer.pattern = p;
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

            // 检查是否为多文件模式
            if !result.analysis_tasks.is_empty() {
                info!("多文件模式: {} 个分析任务", result.analysis_tasks.len());

                // 使用责任链模式执行多文件分析
                let analysis_results =
                    run_multi_file_analysis(&result.analysis_tasks, &plugin_manager, &config)?;

                // 如果有多个结果，合并并生成统一甘特图
                if analysis_results.len() > 1 {
                    merge_and_visualize(&analysis_results, &config, "auto")?;
                }
            } else {
                // 单文件模式（向后兼容）
                let file_to_analyze = result.analyzed_file.context("未找到要分析的文件")?;
                let plugin_name = result.selected_plugin.context("未选择插件")?;

                let plugin = plugin_manager
                    .find_by_name(&plugin_name)
                    .with_context(|| format!("未找到插件: {}", plugin_name))?;

                run_analysis(plugin, &file_to_analyze, &config.local.output_dir)?;
            }
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
                let file_name = input_file
                    .file_name()
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
                            if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                                && glob_match::glob_match(pattern, file_name)
                            {
                                files_to_analyze.push((path.clone(), pattern.clone()));
                                info!("找到本地文件: {}", path.display());
                                break; // 只取第一个匹配的文件
                            }
                        }
                    }
                }
            }

            if files_to_analyze.is_empty() {
                anyhow::bail!("未找到任何要分析的文件");
            }

            info!("共找到 {} 个文件需要分析", files_to_analyze.len());

            // 2. 按主时间轴优先排序（责任链模式）
            // 先找出哪些文件对应主时间轴
            let mut files_with_priority: Vec<(PathBuf, String, bool)> = files_to_analyze
                .into_iter()
                .map(|(path, pattern)| {
                    let is_primary = config
                        .analyzers
                        .iter()
                        .find(|a| a.pattern == pattern)
                        .map(|a| a.is_primary)
                        .unwrap_or(false);
                    (path, pattern, is_primary)
                })
                .collect();

            // 主时间轴优先
            files_with_priority.sort_by_key(|f| std::cmp::Reverse(f.2));

            // 3. 分析每个文件并收集时间线（责任链模式）
            let mut timelines = Vec::new();
            let mut context: Option<AnalyzerContext> = None;

            for (file_path, pattern, is_primary) in &files_with_priority {
                info!(
                    "分析文件: {} (主时间轴: {})",
                    file_path.display(),
                    is_primary
                );

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

                // 准备分析参数（非主时间轴传递上下文）
                let extra_args = if *is_primary {
                    None
                } else {
                    context
                        .as_ref()
                        .and_then(|ctx| serde_json::to_string(ctx).ok())
                        .map(RString::from)
                };

                let analyze_args = AnalyzeArgs {
                    input_file: RString::from(file_path.to_str().context("无效的路径")?),
                    output_dir: RString::from(
                        config.local.output_dir.to_str().context("无效的路径")?,
                    ),
                    extra_args: extra_args.into(),
                };

                // 执行分析
                let result = plugin
                    .plugin
                    .analyze(analyze_args)
                    .into_result()
                    .with_context(|| format!("分析文件失败: {}", file_path.display()))?;

                // 如果是主时间轴，提取轮次时间范围作为上下文
                if *is_primary {
                    let ranges = extract_round_time_ranges(&result.timeline);
                    info!("提取到 {} 个轮次时间范围", ranges.len());
                    context = Some(AnalyzerContext {
                        round_time_ranges: ranges,
                    });
                }

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
                    analyzer_workflow::config::AlignmentStrategy::Timestamp => {
                        AlignmentStrategy::Timestamp
                    }
                    analyzer_workflow::config::AlignmentStrategy::EventBased => {
                        AlignmentStrategy::EventBased
                    }
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

            // 从配置文件动态创建可视化配置
            let vis_config = create_visualization_config(&config);
            let generator = GanttChartGenerator::new(vis_config);

            generator.generate_gantt_chart(
                &merged,
                output_path.to_str().context("无效的路径")?,
                &format!("多文件分析 - {}", prefix),
            )?;

            println!("\n✓ 多文件分析完成！");
            println!("  - 分析文件数: {}", files_with_priority.len());
            println!("  - 合并事件数: {}", merged.events.len());
            println!("  - 甘特图: {}", output_path.display());
        }

        None => {
            // 默认行为：执行 auto 模式
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

            // 检查是否为多文件模式
            if !result.analysis_tasks.is_empty() {
                info!("多文件模式: {} 个分析任务", result.analysis_tasks.len());

                // 使用责任链模式执行多文件分析
                let analysis_results =
                    run_multi_file_analysis(&result.analysis_tasks, &plugin_manager, &config)?;

                // 如果有多个结果，合并并生成统一甘特图
                if analysis_results.len() > 1 {
                    merge_and_visualize(&analysis_results, &config, "auto")?;
                }
            } else {
                // 单文件模式（向后兼容）
                let file_to_analyze = result.analyzed_file.context("未找到要分析的文件")?;
                let plugin_name = result.selected_plugin.context("未选择插件")?;

                let plugin = plugin_manager
                    .find_by_name(&plugin_name)
                    .with_context(|| format!("未找到插件: {}", plugin_name))?;

                run_analysis(plugin, &file_to_analyze, &config.local.output_dir)?;
            }
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
