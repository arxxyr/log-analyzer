//! 字体加载模块
//!
//! 负责加载打包的中文字体文件，避免在其他机器上出现乱码
//! 字体数据直接嵌入到二进制文件中，运行时提取到系统字体目录

use anyhow::{Context, Result};
use plotters::style::FontDesc;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

// 将字体文件直接嵌入到二进制文件中（编译时）
// 这样字体数据就在 .so 文件里，外部看不到原始 .ttf 文件
const FONT_DATA: &[u8] = include_bytes!("../../../../assests/fonts/SarasaTermSCNerd-Regular.ttf");
const FONT_NAME: &str = "SarasaTermSCNerd-Regular.ttf";

/// 全局字体路径缓存
static FONT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 字体加载器
pub struct FontLoader {
    font_available: bool,
}

impl FontLoader {
    /// 创建字体加载器
    ///
    /// 字体数据已经嵌入到二进制文件中，首次调用时会提取到系统字体目录
    pub fn new() -> Result<Self> {
        // 尝试提取并安装字体（只在第一次调用时执行）
        let font_available = Self::ensure_font_installed().is_ok();

        Ok(Self { font_available })
    }

    /// 确保字体已安装（提取嵌入的字体到可访问的目录）
    fn ensure_font_installed() -> Result<PathBuf> {
        // 使用 OnceLock 确保只提取一次
        FONT_PATH
            .get_or_init(|| {
                // 优先使用程序同级的 fonts 目录（相对路径，易于访问）
                let exe_dir = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()));

                let font_dirs = vec![
                    // 1. 程序目录下的 fonts/ 子目录（优先）
                    exe_dir.clone().map(|d| d.join("fonts")),
                    // 2. 当前工作目录下的 fonts/
                    Some(std::path::PathBuf::from("./fonts")),
                    // 3. 程序目录下的隐藏目录
                    exe_dir.map(|d| d.join(".fonts")),
                    // 4. Linux 用户字体目录
                    dirs::home_dir().map(|h| h.join(".local/share/fonts/analyzer")),
                    dirs::home_dir().map(|h| h.join(".fonts/analyzer")),
                    // 5. Windows 用户字体目录
                    dirs::data_local_dir().map(|d| d.join("analyzer/fonts")),
                    // 6. macOS 用户字体目录
                    dirs::home_dir().map(|h| h.join("Library/Fonts/analyzer")),
                    // 7. 临时目录作为最后备选
                    Some(std::env::temp_dir().join(".analyzer_fonts")),
                ];

                for font_dir_opt in font_dirs {
                    if let Some(font_dir) = font_dir_opt {
                        // 尝试创建字体目录
                        if fs::create_dir_all(&font_dir).is_ok() {
                            let font_path = font_dir.join(FONT_NAME);

                            // 如果字体文件不存在或大小不匹配，则写入
                            let need_write = !font_path.exists()
                                || fs::metadata(&font_path)
                                    .map(|m| m.len() as usize != FONT_DATA.len())
                                    .unwrap_or(true);

                            if need_write {
                                if let Ok(_) = fs::write(&font_path, FONT_DATA) {
                                    eprintln!("[字体] 已提取字体到: {:?}", font_path);
                                    // 提取成功后，设置环境变量和刷新缓存
                                    Self::setup_font_environment(&font_dir);
                                    Self::refresh_font_cache(&font_dir);
                                    return Some(font_path);
                                }
                            } else {
                                eprintln!("[字体] 使用已有字体: {:?}", font_path);
                                // 确保环境变量设置正确
                                Self::setup_font_environment(&font_dir);
                                return Some(font_path);
                            }
                        }
                    }
                }

                None
            })
            .clone()
            .context("无法提取字体文件")
    }

    /// 获取嵌入的字体数据
    #[allow(dead_code)]
    pub fn font_data(&self) -> &'static [u8] {
        FONT_DATA
    }

    /// 获取字体文件路径（如果已提取）
    #[allow(dead_code)]
    pub fn font_path(&self) -> Option<&PathBuf> {
        FONT_PATH.get().and_then(|p| p.as_ref())
    }

    /// 设置字体环境变量（让系统能找到字体）
    fn setup_font_environment(font_dir: &std::path::Path) {
        #[cfg(target_os = "linux")]
        {
            // 设置 FONTCONFIG_PATH 环境变量
            // 注意：这只对当前进程及其子进程有效
            if let Some(font_dir_str) = font_dir.to_str() {
                unsafe {
                    std::env::set_var("FONTCONFIG_PATH", font_dir_str);
                }
                eprintln!("[字体] 设置 FONTCONFIG_PATH={}", font_dir_str);
            }
        }
    }

    /// 刷新系统字体缓存（Linux）
    fn refresh_font_cache(font_dir: &std::path::Path) {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            // 为这个特定目录刷新字体缓存
            if let Some(font_dir_str) = font_dir.to_str() {
                eprintln!("[字体] 刷新字体缓存: {}", font_dir_str);
                let _ = Command::new("fc-cache")
                    .arg("-f")
                    .arg(font_dir_str)
                    .output();
            }
        }

        // Windows 和 macOS 通常会自动识别字体文件
    }

    /// 创建字体描述符（用于 plotters）
    ///
    /// # 参数
    /// * `size` - 字体大小
    ///
    /// # 返回
    /// 字体描述符，使用嵌入的中文字体
    pub fn font_desc(&self, size: i32) -> FontDesc<'static> {
        if self.font_available {
            // 使用嵌入的字体数据
            // Sarasa Term SC Nerd 支持中文显示
            // 使用元组形式更简单
            ("Sarasa Term SC Nerd", size as f64).into()
        } else {
            // 回退到系统默认字体（理论上不应该到这里）
            ("sans-serif", size as f64).into()
        }
    }

    /// 检查字体是否可用
    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        self.font_available
    }

    /// 获取字体数据大小（用于调试）
    #[allow(dead_code)]
    pub fn font_size(&self) -> usize {
        FONT_DATA.len()
    }

    /// 获取字体状态信息（用于调试）
    #[allow(dead_code)]
    pub fn status(&self) -> String {
        if self.is_available() {
            format!(
                "使用嵌入字体: Sarasa Term SC Nerd ({}字节)",
                self.font_size()
            )
        } else {
            "使用系统默认字体（嵌入字体不可用）".to_string()
        }
    }
}

impl Default for FontLoader {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
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
        // 应该能成功创建字体描述符
        assert_eq!(desc.get_size(), 14.0);
    }
}
