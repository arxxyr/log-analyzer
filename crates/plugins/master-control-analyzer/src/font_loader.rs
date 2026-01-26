//! 字体加载模块
//!
//! 负责从外部 fonts 目录加载中文字体文件
//! 字体文件需要放在程序同级的 fonts/ 目录下

use anyhow::{Context, Result};
use plotters::style::FontDesc;
use std::path::PathBuf;
use std::sync::OnceLock;

const FONT_NAME: &str = "SarasaTermSCNerd-Regular.ttf";
const FONT_FAMILY: &str = "sarasa";

/// 全局字体路径缓存
static FONT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
/// 字体注册状态
static FONT_REGISTERED: OnceLock<bool> = OnceLock::new();

/// 字体加载器
pub struct FontLoader {
    font_available: bool,
}

impl FontLoader {
    /// 创建字体加载器
    ///
    /// 从外部 fonts 目录查找字体文件并注册到 plotters
    pub fn new() -> Result<Self> {
        let font_path = Self::find_font();
        let font_available = font_path.is_ok();

        // 注册字体到 plotters
        if let Ok(path) = font_path {
            Self::register_font(&path);
        }

        Ok(Self { font_available })
    }

    /// 查找外部字体文件
    fn find_font() -> Result<PathBuf> {
        FONT_PATH
            .get_or_init(|| {
                // 获取程序所在目录
                let exe_dir = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()));

                // 字体搜索路径（按优先级）
                let font_dirs = vec![
                    // 1. 程序目录下的 fonts/ 子目录（优先）
                    exe_dir.clone().map(|d| d.join("fonts")),
                    // 2. 当前工作目录下的 fonts/
                    Some(std::path::PathBuf::from("./fonts")),
                    // 3. 程序目录下的隐藏目录
                    exe_dir.map(|d| d.join(".fonts")),
                    // 4. Linux 用户字体目录
                    dirs::home_dir().map(|h| h.join(".local/share/fonts")),
                    dirs::home_dir().map(|h| h.join(".fonts")),
                    // 5. Windows 用户字体目录
                    dirs::font_dir(),
                    dirs::data_local_dir().map(|d| d.join("analyzer/fonts")),
                    // 6. macOS 用户字体目录
                    dirs::home_dir().map(|h| h.join("Library/Fonts")),
                ];

                for font_dir in font_dirs.into_iter().flatten() {
                    let font_path = font_dir.join(FONT_NAME);
                    if font_path.exists() {
                        eprintln!("[字体] 使用字体: {:?}", font_path);
                        // 设置环境变量和刷新缓存
                        Self::setup_font_environment(&font_dir);
                        Self::refresh_font_cache(&font_dir);
                        return Some(font_path);
                    }
                }

                eprintln!("[字体] 警告: 未找到字体文件 {}", FONT_NAME);
                eprintln!("[字体] 请将字体文件放置在以下位置之一:");
                eprintln!("       - 程序目录/fonts/{}", FONT_NAME);
                eprintln!("       - 当前目录/fonts/{}", FONT_NAME);
                None
            })
            .clone()
            .context("未找到字体文件")
    }

    /// 注册字体到 plotters
    fn register_font(font_path: &PathBuf) {
        FONT_REGISTERED.get_or_init(|| {
            // 读取字体文件数据
            match std::fs::read(font_path) {
                Ok(font_data) => {
                    // 使用 plotters 的 register_font 函数注册字体
                    // 需要将数据泄漏为 'static 生命周期
                    use plotters::style::FontStyle;
                    let font_data_static: &'static [u8] = Box::leak(font_data.into_boxed_slice());
                    match plotters::style::register_font(
                        FONT_FAMILY,
                        FontStyle::Normal,
                        font_data_static,
                    ) {
                        Ok(()) => {
                            eprintln!("[字体] 已注册字体: {}", FONT_FAMILY);
                            true
                        }
                        Err(_) => {
                            eprintln!("[字体] 注册字体失败");
                            false
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[字体] 读取字体文件失败: {}", e);
                    false
                }
            }
        });
    }

    /// 获取字体文件路径（如果已找到）
    #[allow(dead_code)]
    pub fn font_path(&self) -> Option<&PathBuf> {
        FONT_PATH.get().and_then(|p| p.as_ref())
    }

    /// 设置字体环境变量（让系统能找到字体）
    fn setup_font_environment(_font_dir: &std::path::Path) {
        #[cfg(target_os = "linux")]
        {
            // 设置 FONTCONFIG_PATH 环境变量
            if let Some(font_dir_str) = _font_dir.to_str() {
                unsafe {
                    std::env::set_var("FONTCONFIG_PATH", font_dir_str);
                }
            }
        }
    }

    /// 刷新系统字体缓存（Linux）
    fn refresh_font_cache(_font_dir: &std::path::Path) {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            if let Some(font_dir_str) = _font_dir.to_str() {
                let _ = Command::new("fc-cache")
                    .arg("-f")
                    .arg(font_dir_str)
                    .output();
            }
        }
    }

    /// 创建字体描述符（用于 plotters）
    ///
    /// # 参数
    /// * `size` - 字体大小
    ///
    /// # 返回
    /// 字体描述符
    pub fn font_desc(&self, size: i32) -> FontDesc<'static> {
        if self.font_available {
            // 使用已注册的字体（支持中文）
            (FONT_FAMILY, size as f64).into()
        } else {
            // 回退到系统默认字体
            ("sans-serif", size as f64).into()
        }
    }

    /// 检查字体是否可用
    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        self.font_available
    }
}

impl Default for FontLoader {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            font_available: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_loader_creation() {
        let loader = FontLoader::new();
        assert!(loader.is_ok());
    }

    #[test]
    fn test_font_desc() {
        let loader = FontLoader::default();
        let desc = loader.font_desc(14);
        assert_eq!(desc.get_size(), 14.0);
    }
}
