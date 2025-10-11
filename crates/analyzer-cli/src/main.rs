//! Analyzer CLI - 通用日志分析器命令行工具
//!
//! 这是一个基于插件的日志分析器框架，支持动态加载不同类型的分析器插件。
//!
//! # 架构
//!
//! - 扫描 plugins/ 目录，加载所有可用插件
//! - 根据文件扩展名自动选择合适的插件
//! - 调用插件执行分析，生成结果

use abi_stable::{library::RootModule, std_types::{RBox, RString}};
use analyzer_core::*;
use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

// ============================================================================
// 命令行参数
// ============================================================================

/// 通用日志分析器 CLI
#[derive(Parser, Debug)]
#[command(name = "analyzer")]
#[command(about = "通用日志分析器（支持插件扩展）", long_about = None)]
struct CliArgs {
    /// 输入日志文件路径
    #[arg(short = 'i', long = "input", value_name = "FILE", required_unless_present = "list_plugins")]
    input: Option<String>,

    /// 输出目录路径
    #[arg(short = 'o', long = "output", value_name = "DIR", default_value = "output")]
    output: String,

    /// 指定插件名称（可选，不指定则根据文件扩展名自动选择）
    #[arg(short = 'p', long = "plugin", value_name = "NAME")]
    plugin: Option<String>,

    /// 插件目录（可选，默认为可执行文件同级的 plugins 目录）
    #[arg(long = "plugin-dir", value_name = "DIR")]
    plugin_dir: Option<String>,

    /// 列出所有可用插件
    #[arg(short = 'l', long = "list")]
    list_plugins: bool,

    /// 显示详细输出
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

// ============================================================================
// 插件管理
// ============================================================================

/// 插件信息
struct PluginInfo {
    name: String,
    path: PathBuf,
    metadata: PluginMetadata,
    plugin: AnalyzerPlugin_TO<'static, RBox<()>>,
}

/// 插件管理器
struct PluginManager {
    plugins: Vec<PluginInfo>,
}

impl PluginManager {
    /// 创建新的插件管理器
    fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// 扫描并加载指定目录下的所有插件
    fn load_plugins(&mut self, plugin_dir: &Path) -> Result<()> {
        if !plugin_dir.exists() {
            anyhow::bail!("插件目录不存在: {}", plugin_dir.display());
        }

        println!("正在扫描插件目录: {}", plugin_dir.display());

        // 遍历插件目录
        for entry in std::fs::read_dir(plugin_dir)? {
            let entry = entry?;
            let path = entry.path();

            // 检查是否是动态库文件
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

                // 尝试加载插件
                match self.load_plugin(&path) {
                    Ok(plugin_info) => {
                        println!("  ✓ 加载插件: {}", plugin_info.name);
                        self.plugins.push(plugin_info);
                    }
                    Err(e) => {
                        eprintln!("  ✗ 加载失败: {} - {}", path.display(), e);
                    }
                }
            }
        }

        if self.plugins.is_empty() {
            anyhow::bail!("未找到任何可用插件");
        }

        println!("成功加载 {} 个插件\n", self.plugins.len());
        Ok(())
    }

    /// 加载单个插件
    fn load_plugin(&self, path: &Path) -> Result<PluginInfo> {
        // 使用 abi_stable 的 RootModule::load_from_file 加载插件
        let module = AnalyzerPluginModule_Ref::load_from_file(path)
            .map_err(|e| anyhow::anyhow!("无法加载插件 {}: {:?}", path.display(), e))?;

        // 创建插件实例
        let plugin = module.create_plugin()();

        // 获取元信息
        let metadata = plugin.metadata();
        let name = metadata.name.to_string();

        Ok(PluginInfo {
            name,
            path: path.to_path_buf(),
            metadata,
            plugin,
        })
    }

    /// 根据名称查找插件
    fn find_by_name(&self, name: &str) -> Option<&PluginInfo> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// 根据文件扩展名查找合适的插件
    fn find_by_extension(&self, file_path: &str) -> Option<&PluginInfo> {
        let path = Path::new(file_path);
        let ext = path.extension()?.to_str()?;
        let ext_with_dot = format!(".{}", ext);

        self.plugins.iter().find(|p| {
            p.metadata
                .supported_extensions
                .iter()
                .any(|e| e.as_str() == ext_with_dot)
        })
    }

    /// 列出所有插件
    fn list_plugins(&self) {
        println!("可用插件列表:\n");
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
// 主程序
// ============================================================================

fn main() -> Result<()> {
    let args = CliArgs::parse();

    // 确定插件目录
    let plugin_dir = if let Some(dir) = args.plugin_dir {
        PathBuf::from(dir)
    } else {
        // 默认为可执行文件同级的 plugins 目录
        let exe_dir = std::env::current_exe()?
            .parent()
            .context("无法获取可执行文件目录")?
            .to_path_buf();
        exe_dir.join("plugins")
    };

    // 加载所有插件
    let mut manager = PluginManager::new();
    manager.load_plugins(&plugin_dir)?;

    // 如果只是列出插件，则显示后退出
    if args.list_plugins {
        manager.list_plugins();
        return Ok(());
    }

    // 获取输入文件（此时已确保存在）
    let input_file = args.input.as_ref().unwrap();

    // 选择插件
    let plugin = if let Some(plugin_name) = &args.plugin {
        // 用户指定了插件名称
        manager
            .find_by_name(plugin_name)
            .with_context(|| format!("未找到插件: {}", plugin_name))?
    } else {
        // 根据文件扩展名自动选择
        manager
            .find_by_extension(input_file)
            .context("无法根据文件扩展名找到合适的插件，请使用 --plugin 手动指定")?
    };

    println!("使用插件: {} v{}", plugin.metadata.name, plugin.metadata.version);
    println!("正在分析: {}", input_file);
    println!("输出目录: {}\n", args.output);

    // 准备分析参数
    let analyze_args = AnalyzeArgs {
        input_file: RString::from(input_file.as_str()),
        output_dir: RString::from(args.output.as_str()),
        extra_args: None.into(),
    };

    // 执行分析
    let result = plugin
        .plugin
        .analyze(analyze_args)
        .into_result()
        .context("插件分析失败")?;

    // 显示结果
    println!("{}", result.summary);

    if args.verbose {
        println!("\n生成的文件:");
        for file in result.output_files.iter() {
            println!("  - {} ({}): {}", file.path, file.file_type, file.description);
        }
    }

    println!("\n分析完成！");
    Ok(())
}
