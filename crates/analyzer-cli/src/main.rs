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

// i18n 初始化
rust_i18n::i18n!("locales", fallback = "zh-CN");

use abi_stable::std_types::RString;
use analyzer_core::timeline::Track;
use analyzer_core::*;
use analyzer_merger::{AlignmentStrategy, MergeConfig, TimelineMerger};
use analyzer_visualizer::{GanttChartGenerator, VisualizationConfig};
use analyzer_workflow::{AnalysisTask, AnalyzerConfig, WorkflowOrchestrator};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

/// 当前语言设置（全局，线程安全）
static CURRENT_LOCALE: OnceLock<String> = OnceLock::new();

/// 获取当前语言设置
fn get_current_locale() -> String {
    CURRENT_LOCALE
        .get()
        .cloned()
        .unwrap_or_else(|| "zh-CN".to_string())
}

/// 从环境变量检测语言设置
/// LANG 环境变量 > 默认 zh-CN
fn detect_locale_from_env() -> String {
    if let Ok(lang) = std::env::var("LANG") {
        return normalize_locale(&lang);
    }
    "zh-CN".to_string()
}

/// 规范化语言代码
fn normalize_locale(lang: &str) -> String {
    let lang_lower = lang.to_lowercase();
    if lang_lower.starts_with("en") {
        "en".to_string()
    } else if lang_lower.starts_with("zh")
        || lang_lower.contains("cn")
        || lang_lower.contains("chinese")
    {
        "zh-CN".to_string()
    } else {
        // 未知语言，默认中文
        "zh-CN".to_string()
    }
}

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

/// 通用日志分析器 CLI (v0.4.0)
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

    /// 语言设置 (zh-CN, en)
    #[arg(long, global = true)]
    lang: Option<String>,

    /// 每个模式最多分析的文件数量（覆盖配置文件中的 max_files）
    #[arg(short = 'n', long, global = true)]
    max_files: Option<usize>,
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
            warn!(
                "{}",
                t!(
                    "msg.plugin_dir_not_exist",
                    path = plugin_dir.display().to_string()
                )
            );
            return Ok(());
        }

        info!(
            "{}",
            t!(
                "msg.scan_plugin_dir",
                path = plugin_dir.display().to_string()
            )
        );

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
                    info!(
                        "{}",
                        t!("msg.load_plugin_success", name = &plugin_info.name)
                    );
                    self.plugins.push(plugin_info);
                }
                Err(e) => {
                    warn!(
                        "{}",
                        t!(
                            "msg.load_plugin_failed",
                            path = path.display().to_string(),
                            error = e.to_string()
                        )
                    );
                }
            }
        }

        if self.plugins.is_empty() {
            anyhow::bail!("{}", t!("msg.no_plugins_found"));
        }

        info!("{}", t!("msg.plugins_loaded", count = self.plugins.len()));
        Ok(())
    }

    fn load_plugin(&self, path: &Path) -> Result<PluginInfo> {
        use abi_stable::library::lib_header_from_path;

        // 使用 abi_stable 的 lib_header_from_path 加载插件
        // 这会正确处理 LibHeader 结构并获取根模块
        let lib_header = lib_header_from_path(path).map_err(|e| {
            anyhow::anyhow!(
                "{}",
                t!(
                    "err.load_plugin_lib",
                    path = path.display().to_string(),
                    error = format!("{:?}", e)
                )
            )
        })?;

        // 初始化根模块并检查 ABI 兼容性
        let module_ref = lib_header
            .init_root_module::<AnalyzerPluginModule_Ref>()
            .map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    t!(
                        "err.init_plugin_module",
                        path = path.display().to_string(),
                        error = format!("{:?}", e)
                    )
                )
            })?;

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
        // 精确匹配优先
        self.plugins.iter().find(|p| p.name == name).or_else(|| {
            // 回退到模糊匹配，但只在恰好一个候选时返回
            let candidates: Vec<_> = self
                .plugins
                .iter()
                .filter(|p| p.name.contains(name))
                .collect();
            if candidates.len() == 1 {
                Some(candidates[0])
            } else {
                None
            }
        })
    }

    fn list_plugins(&self) {
        println!("\n{}\n", t!("msg.plugin_list_title"));
        for plugin in &self.plugins {
            println!(
                "{}",
                t!("msg.plugin_name", name = plugin.metadata.name.as_str())
            );
            println!(
                "{}",
                t!(
                    "msg.plugin_version",
                    version = plugin.metadata.version.as_str()
                )
            );
            println!(
                "{}",
                t!(
                    "msg.plugin_author",
                    author = plugin.metadata.author.as_str()
                )
            );
            println!(
                "{}",
                t!(
                    "msg.plugin_description",
                    description = plugin.metadata.description.as_str()
                )
            );
            print!("{}", t!("msg.plugin_extensions"));
            for (i, ext) in plugin.metadata.supported_extensions.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}", ext);
            }
            println!(
                "\n{}",
                t!("msg.plugin_path", path = plugin.path.display().to_string())
            );
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
        "{}",
        t!(
            "msg.using_plugin",
            name = plugin.metadata.name.as_str(),
            version = plugin.metadata.version.as_str()
        )
    );
    info!(
        "{}",
        t!(
            "msg.analyzing_file",
            path = input_file.display().to_string()
        )
    );
    info!(
        "{}",
        t!("msg.output_dir", path = output_dir.display().to_string())
    );

    // 准备分析参数
    let extra_args = context
        .and_then(|ctx| serde_json::to_string(ctx).ok())
        .map(RString::from);

    let analyze_args = AnalyzeArgs {
        input_file: RString::from(
            input_file
                .to_str()
                .context(t!("err.invalid_path").to_string())?,
        ),
        output_dir: RString::from(
            output_dir
                .to_str()
                .context(t!("err.invalid_path").to_string())?,
        ),
        extra_args: extra_args.into(),
        locale: RString::from(get_current_locale()),
    };

    // 执行分析
    let result = plugin
        .plugin
        .analyze(analyze_args)
        .into_result()
        .context(t!("err.plugin_analysis_failed").to_string())?;

    // 显示结果
    println!("\n{}", result.summary);
    println!("\n{}:", t!("msg.generated_files"));
    for file in result.output_files.iter() {
        println!(
            "  - {} ({}): {}",
            file.path, file.file_type, file.description
        );
    }

    println!("\n{}", t!("msg.analysis_complete"));
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

/// 在指定输出目录生成单文件的汇总甘特图 auto_merged_gantt.png
fn generate_auto_gantt(
    result: &AnalyzeResult,
    output_dir: &Path,
    config: &AnalyzerConfig,
) -> Result<()> {
    let output_path = output_dir.join("auto_merged_gantt.png");

    let merge_config = MergeConfig {
        primary_source: config.multi_file.alignment.primary_source.clone(),
        time_tolerance: config.multi_file.alignment.time_tolerance,
        alignment_strategy: AlignmentStrategy::Timestamp,
        track_priority: config.multi_file.track_priority.clone(),
    };

    let merged = TimelineMerger::new(merge_config).merge(vec![result.timeline.clone()])?;

    let generator = GanttChartGenerator::new(create_visualization_config(config));
    generator.generate_gantt_chart(
        &merged,
        output_path
            .to_str()
            .context(t!("err.invalid_path").to_string())?,
        t!("msg.multi_analysis_complete").as_ref(),
    )?;

    info!("已生成: {}", output_path.display());
    Ok(())
}

/// 检测是否存在同 pattern 的多个 task（批量模式）
fn has_batch_tasks(tasks: &[AnalysisTask]) -> bool {
    let mut pattern_counts = std::collections::HashMap::new();
    for task in tasks {
        *pattern_counts.entry(&task.pattern).or_insert(0usize) += 1;
    }
    pattern_counts.values().any(|&count| count > 1)
}

/// 运行多文件分析（责任链模式）
fn run_multi_file_analysis(
    tasks: &[AnalysisTask],
    plugin_manager: &PluginManager,
    config: &AnalyzerConfig,
) -> Result<Vec<AnalyzeResult>> {
    let mut results = Vec::new();
    let mut context: Option<AnalyzerContext> = None;
    let is_batch = has_batch_tasks(tasks);

    if is_batch {
        println!("\n{}", t!("msg.batch_analysis_start", count = tasks.len()));
    }

    // 按主时间轴优先排序：先分析 is_primary=true 的任务
    let mut sorted_tasks: Vec<_> = tasks.to_vec();
    sorted_tasks.sort_by_key(|t| std::cmp::Reverse(t.is_primary));

    for task in &sorted_tasks {
        info!(
            "{}",
            t!(
                "msg.analyzing_with_primary",
                path = &task.file_name,
                is_primary = task.is_primary
            )
        );

        let plugin = plugin_manager
            .find_by_name(&task.plugin_name)
            .with_context(|| t!("err.plugin_not_found", name = &task.plugin_name).to_string())?;

        // 批量模式下，为每个文件创建独立的输出子目录
        let output_dir = if is_batch {
            let file_stem = Path::new(&task.file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&task.file_name);
            let sub_dir = config.local.output_dir.join(file_stem);
            fs::create_dir_all(&sub_dir)?;
            println!(
                "{}",
                t!(
                    "msg.batch_file_output",
                    name = &task.file_name,
                    dir = sub_dir.display().to_string()
                )
            );
            sub_dir
        } else {
            config.local.output_dir.clone()
        };

        // 执行分析（主时间轴不需要上下文，后续分析器需要）
        let result = if task.is_primary {
            run_analysis(plugin, &task.local_file, &output_dir)?
        } else {
            run_analysis_with_context(plugin, &task.local_file, &output_dir, context.as_ref())?
        };

        // 如果是主时间轴，提取轮次时间范围作为上下文
        if task.is_primary {
            let ranges = extract_round_time_ranges(&result.timeline);
            info!("{}", t!("msg.extracted_rounds", count = ranges.len()));
            context = Some(AnalyzerContext {
                round_time_ranges: ranges,
            });
        }

        // 批量模式：在各自子目录生成汇总甘特图
        if is_batch && let Err(e) = generate_auto_gantt(&result, &output_dir, config) {
            warn!("auto_merged_gantt.png 生成失败（非致命）: {}", e);
        }

        results.push(result);
    }

    if is_batch {
        println!(
            "\n{}",
            t!("msg.batch_analysis_complete", count = results.len())
        );
    }

    Ok(results)
}

/// 清理旧输出文件（png、csv 及批量子目录）
fn cleanup_old_output(output_dir: &Path) -> Result<()> {
    if !output_dir.exists() {
        return Ok(());
    }
    info!("{}", t!("msg.cleanup_old_output"));
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str());
            if matches!(ext, Some("png") | Some("csv")) {
                let _ = fs::remove_file(path);
            }
        } else if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        }
    }
    Ok(())
}

/// 执行自动工作流（Auto 和默认模式共用逻辑）
fn run_auto_mode(config: &AnalyzerConfig, plugin_manager: &PluginManager) -> Result<()> {
    // 清理旧输出
    if config.local.cleanup_old_output {
        cleanup_old_output(&config.local.output_dir)?;
    }

    // 执行自动工作流
    let mut orchestrator = WorkflowOrchestrator::new(config.clone())?;
    let result = orchestrator.run_auto()?;

    if !result.success {
        error!("{}", t!("msg.workflow_failed"));
        for err in &result.errors {
            error!("  - {}", err);
        }
        anyhow::bail!("{}", t!("err.workflow_failed"));
    }

    // 检查是否为多文件模式
    if !result.analysis_tasks.is_empty() {
        info!(
            "{}",
            t!("msg.multi_file_mode", count = result.analysis_tasks.len())
        );

        run_multi_file_analysis(&result.analysis_tasks, plugin_manager, config)?;
    } else {
        // 单文件模式（向后兼容）
        let file_to_analyze = result
            .analyzed_file
            .context(t!("err.no_file_to_analyze").to_string())?;
        let plugin_name = result
            .selected_plugin
            .context(t!("err.no_plugin_selected").to_string())?;

        let plugin = plugin_manager
            .find_by_name(&plugin_name)
            .with_context(|| t!("err.plugin_not_found", name = &plugin_name).to_string())?;

        run_analysis(plugin, &file_to_analyze, &config.local.output_dir)?;
    }

    Ok(())
}

/// 合并多个 Timeline 并生成统一甘特图
fn merge_and_visualize(
    results: &[AnalyzeResult],
    config: &AnalyzerConfig,
    output_prefix: &str,
) -> Result<()> {
    if results.len() <= 1 {
        info!("{}", t!("msg.only_one_result"));
        return Ok(());
    }

    info!(
        "{}",
        t!("msg.merging_timelines_count", count = results.len())
    );

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

    info!("{}", t!("msg.merge_complete", count = merged.events.len()));

    // 生成统一甘特图
    let output_path = config
        .local
        .output_dir
        .join(format!("{}_merged_gantt.png", output_prefix));

    info!(
        "{}",
        t!(
            "msg.generating_unified_gantt",
            path = output_path.display().to_string()
        )
    );

    // 从配置文件动态创建可视化配置
    let vis_config = create_visualization_config(config);
    let generator = GanttChartGenerator::new(vis_config);

    generator.generate_gantt_chart(
        &merged,
        output_path
            .to_str()
            .context(t!("err.invalid_path").to_string())?,
        t!("msg.multi_analysis_complete").as_ref(),
    )?;

    println!("\n{}", t!("msg.multi_analysis_complete"));
    println!("{}", t!("msg.analyzed_file_count", count = results.len()));
    println!(
        "{}",
        t!("msg.merged_event_count", count = merged.events.len())
    );
    println!(
        "{}",
        t!(
            "msg.unified_gantt_path",
            path = output_path.display().to_string()
        )
    );

    Ok(())
}

// ============================================================================
// 主程序
// ============================================================================

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 设置语言：--lang 参数 > LANG 环境变量 > 默认 zh-CN
    let locale = match &cli.lang {
        Some(lang) => normalize_locale(lang),
        None => detect_locale_from_env(),
    };
    rust_i18n::set_locale(&locale);
    let _ = CURRENT_LOCALE.set(locale);

    // 初始化日志
    let log_level = if cli.verbose { "debug" } else { &cli.log_level };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level)),
        )
        .with_target(false)
        .with_level(true)
        .init();

    // 预检字体可用性（提前检测，字体不可用时仅警告不中断）
    analyzer_visualizer::check_font_availability();

    debug!("配置文件路径: {:?}", cli.config);

    // 加载配置
    let config = if cli.config.exists() {
        info!(
            "{}",
            t!("msg.load_config", path = cli.config.display().to_string())
        );
        AnalyzerConfig::load_from_file(&cli.config)?
    } else {
        warn!("{}", t!("msg.config_not_exist"));
        AnalyzerConfig::default()
    };

    // 如果指定了 -n 参数，覆盖所有分析器的 max_files 配置
    let mut config = config;
    if let Some(n) = cli.max_files {
        for analyzer in &mut config.analyzers {
            analyzer.max_files = n;
        }
        // max_files > 1 时自动启用 multi_file 模式
        if n > 1 {
            config.multi_file.enabled = true;
        }
    }

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
            println!("{}", t!("msg.config_valid"));
            println!("\n{}:", t!("msg.config_summary"));
            println!(
                "{}",
                if config.remote.enabled {
                    t!("msg.remote_enabled")
                } else {
                    t!("msg.remote_disabled")
                }
            );
            if config.remote.enabled {
                println!(
                    "{}",
                    t!(
                        "msg.remote_host",
                        user = &config.remote.user,
                        host = &config.remote.host,
                        port = config.remote.port
                    )
                );
            }
            println!(
                "{}",
                t!(
                    "msg.local_log_dir",
                    path = config.local.log_dir.display().to_string()
                )
            );
            println!(
                "{}",
                t!(
                    "msg.output_directory",
                    path = config.local.output_dir.display().to_string()
                )
            );
            println!(
                "{}",
                t!(
                    "msg.plugin_directory",
                    path = config.local.plugin_dir.display().to_string()
                )
            );
            println!(
                "{}",
                t!("msg.analyzer_count", count = config.analyzers.len())
            );
            return Ok(());
        }

        Some(Commands::ListRemote { pattern }) => {
            let mut orchestrator = WorkflowOrchestrator::new(config)?;
            let files = orchestrator.list_remote_logs(pattern.as_deref())?;

            println!("\n{}:\n", t!("msg.remote_files_title"));
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
                println!(
                    "{}",
                    t!("msg.file_downloaded", path = out.display().to_string())
                );
            } else {
                println!(
                    "{}",
                    t!(
                        "msg.file_downloaded",
                        path = local_path.display().to_string()
                    )
                );
            }
            return Ok(());
        }

        Some(Commands::Auto { pattern, output }) => {
            let mut config = config;
            if let Some(p) = pattern
                && let Some(first_analyzer) = config.analyzers.first_mut()
            {
                first_analyzer.pattern = p;
            }
            if let Some(o) = output {
                config.local.output_dir = o;
            }

            run_auto_mode(&config, &plugin_manager)?;
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
                anyhow::bail!(
                    "{}",
                    t!(
                        "err.file_not_exist",
                        path = input_file.display().to_string()
                    )
                );
            }

            // 选择插件
            let plugin = if let Some(name) = plugin_name {
                plugin_manager
                    .find_by_name(&name)
                    .with_context(|| t!("err.plugin_not_found", name = &name).to_string())?
            } else {
                // 通过 workflow selector 自动选择
                let orchestrator = WorkflowOrchestrator::new(config.clone())?;
                let file_name = input_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context(t!("err.invalid_path").to_string())?;

                let mapping = orchestrator
                    .selector()
                    .select_plugin(file_name)
                    .context(t!("err.cannot_auto_select_plugin").to_string())?;

                plugin_manager
                    .find_by_name(&mapping.plugin)
                    .with_context(|| {
                        t!("err.plugin_not_found", name = &mapping.plugin).to_string()
                    })?
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
                anyhow::bail!("{}", t!("msg.multi_file_analysis_disabled"));
            }

            info!("{}", t!("msg.multi_file_start"));

            // 1. 准备文件列表
            let mut files_to_analyze: Vec<(PathBuf, String)> = Vec::new(); // (file_path, pattern)

            if auto_download && config.remote.enabled {
                // 从远程下载多个文件
                let mut orchestrator = WorkflowOrchestrator::new(config.clone())?;

                for pattern in &config.multi_file.auto_patterns {
                    info!("{}", t!("msg.finding_remote_file", pattern = pattern));
                    let remote_files = orchestrator.list_remote_logs(Some(pattern))?;

                    if let Some(file_info) = remote_files.first() {
                        info!("{}", t!("msg.downloading", name = &file_info.name));
                        let local_path = orchestrator.download_only(&file_info.name)?;
                        files_to_analyze.push((local_path, pattern.clone()));
                    } else {
                        warn!("{}", t!("msg.no_matching_file", pattern = pattern));
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
                                && glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file_name))
                            {
                                files_to_analyze.push((path.clone(), pattern.clone()));
                                info!(
                                    "{}",
                                    t!("msg.found_local_file", path = path.display().to_string())
                                );
                                break; // 只取第一个匹配的文件
                            }
                        }
                    }
                }
            }

            if files_to_analyze.is_empty() {
                anyhow::bail!("{}", t!("msg.no_files_to_analyze"));
            }

            info!(
                "{}",
                t!("msg.files_to_analyze", count = files_to_analyze.len())
            );

            // 2. 构建 AnalysisTask 列表，复用 run_multi_file_analysis
            let tasks: Vec<AnalysisTask> = files_to_analyze
                .into_iter()
                .map(|(path, pattern)| {
                    let analyzer = config.analyzers.iter().find(|a| a.pattern == pattern);
                    let plugin_name = analyzer.map(|a| a.plugin.clone()).unwrap_or_default();
                    let is_primary = analyzer.map(|a| a.is_primary).unwrap_or(false);
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_string();

                    AnalysisTask {
                        file_name,
                        local_file: path,
                        plugin_name,
                        pattern,
                        is_primary,
                    }
                })
                .collect();

            // 3. 分析所有文件（责任链模式）
            let analysis_results = run_multi_file_analysis(&tasks, &plugin_manager, &config)?;

            // 4. 合并并生成统一甘特图
            merge_and_visualize(&analysis_results, &config, &prefix)?;
        }

        None => {
            // 默认行为：执行 auto 模式
            info!("{}", t!("msg.no_command_auto"));
            run_auto_mode(&config, &plugin_manager)?;
        }
    }

    Ok(())
}
