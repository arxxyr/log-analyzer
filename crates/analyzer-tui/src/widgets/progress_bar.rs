//! 进度条组件

use crate::state::ProgressInfo;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Gauge},
    Frame,
};

/// 渲染进度条
pub fn render(frame: &mut Frame, area: Rect, progress: &ProgressInfo) {
    let percentage = progress.percentage as u16;

    // 构建标签
    let label = if let Some(speed) = progress.speed {
        format!(
            "{}/{} ({:.1}%) - {}/s",
            format_bytes(progress.current),
            format_bytes(progress.total),
            progress.percentage,
            format_bytes(speed as u64)
        )
    } else {
        format!(
            "{}/{} ({:.1}%)",
            format_bytes(progress.current),
            format_bytes(progress.total),
            progress.percentage
        )
    };

    let gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Black),
        )
        .percent(percentage)
        .label(label)
        .use_unicode(true);

    frame.render_widget(gauge, area);
}

/// 格式化字节数
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}
