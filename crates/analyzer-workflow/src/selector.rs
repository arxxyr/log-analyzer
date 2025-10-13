//! 插件选择器模块
//!
//! 根据文件名和配置规则自动选择合适的分析器插件

use crate::config::AnalyzerMapping;
use glob::Pattern;
use tracing::{debug, info};

/// 插件选择器
pub struct PluginSelector {
    mappings: Vec<AnalyzerMapping>,
}

impl PluginSelector {
    /// 创建插件选择器
    pub fn new(mut mappings: Vec<AnalyzerMapping>) -> Self {
        // 按优先级排序（高优先级在前）
        mappings.sort_by(|a, b| b.priority.cmp(&a.priority));

        Self { mappings }
    }

    /// 根据文件名选择插件
    pub fn select_plugin(&self, file_name: &str) -> Option<&AnalyzerMapping> {
        debug!("为文件 '{}' 选择插件", file_name);

        for mapping in &self.mappings {
            if !mapping.enabled {
                continue;
            }

            if Self::matches_pattern(&mapping.pattern, file_name) {
                info!(
                    "匹配到插件: {} (模式: {}, 优先级: {})",
                    mapping.plugin, mapping.pattern, mapping.priority
                );
                return Some(mapping);
            }
        }

        debug!("没有找到匹配的插件");
        None
    }

    /// 列出所有启用的插件
    pub fn list_enabled(&self) -> Vec<&AnalyzerMapping> {
        self.mappings.iter().filter(|m| m.enabled).collect()
    }

    /// 根据名称查找插件
    pub fn find_by_name(&self, name: &str) -> Option<&AnalyzerMapping> {
        self.mappings
            .iter()
            .find(|m| m.name == name || m.plugin == name)
    }

    /// 检查文件名是否匹配模式
    fn matches_pattern(pattern: &str, file_name: &str) -> bool {
        match Pattern::new(pattern) {
            Ok(p) => p.matches(file_name),
            Err(e) => {
                debug!("无效的 glob 模式 '{}': {}", pattern, e);
                false
            }
        }
    }

    /// 获取所有映射
    pub fn mappings(&self) -> &[AnalyzerMapping] {
        &self.mappings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_mappings() -> Vec<AnalyzerMapping> {
        vec![
            AnalyzerMapping {
                name: "master-control".to_string(),
                pattern: "master_control_*.log".to_string(),
                plugin: "master-control-analyzer".to_string(),
                description: "主控分析器".to_string(),
                enabled: true,
                priority: 10,
                config: None,
            },
            AnalyzerMapping {
                name: "navigation".to_string(),
                pattern: "navigation_*.log".to_string(),
                plugin: "navigation-analyzer".to_string(),
                description: "导航分析器".to_string(),
                enabled: true,
                priority: 5,
                config: None,
            },
            AnalyzerMapping {
                name: "generic".to_string(),
                pattern: "*.log".to_string(),
                plugin: "generic-analyzer".to_string(),
                description: "通用分析器".to_string(),
                enabled: true,
                priority: -1,
                config: None,
            },
        ]
    }

    #[test]
    fn test_select_plugin() {
        let selector = PluginSelector::new(create_test_mappings());

        // 精确匹配
        let result = selector.select_plugin("master_control_123.log");
        assert!(result.is_some());
        assert_eq!(result.unwrap().plugin, "master-control-analyzer");

        // 另一个匹配
        let result = selector.select_plugin("navigation_456.log");
        assert!(result.is_some());
        assert_eq!(result.unwrap().plugin, "navigation-analyzer");

        // 兜底匹配
        let result = selector.select_plugin("unknown_file.log");
        assert!(result.is_some());
        assert_eq!(result.unwrap().plugin, "generic-analyzer");
    }

    #[test]
    fn test_priority() {
        let selector = PluginSelector::new(create_test_mappings());

        // master_control_*.log 应该优先于 *.log
        let result = selector.select_plugin("master_control_123.log");
        assert_eq!(result.unwrap().priority, 10);
    }

    #[test]
    fn test_disabled_plugin() {
        let mut mappings = create_test_mappings();
        mappings[0].enabled = false; // 禁用 master-control

        let selector = PluginSelector::new(mappings);

        // 应该匹配到通用分析器
        let result = selector.select_plugin("master_control_123.log");
        assert!(result.is_some());
        assert_eq!(result.unwrap().plugin, "generic-analyzer");
    }

    #[test]
    fn test_list_enabled() {
        let selector = PluginSelector::new(create_test_mappings());
        let enabled = selector.list_enabled();

        assert_eq!(enabled.len(), 3);
    }

    #[test]
    fn test_find_by_name() {
        let selector = PluginSelector::new(create_test_mappings());

        let result = selector.find_by_name("master-control");
        assert!(result.is_some());
        assert_eq!(result.unwrap().plugin, "master-control-analyzer");

        let result = selector.find_by_name("master-control-analyzer");
        assert!(result.is_some());
    }
}
