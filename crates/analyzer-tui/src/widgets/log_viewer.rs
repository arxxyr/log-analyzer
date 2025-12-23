//! 日志查看器组件

use crate::state::{AppState, LogLevel};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// 渲染日志查看器
pub fn render(frame: &mut Frame, area: Rect, state: &AppState, scroll_offset: u16) {
    // 计算可显示的行数
    let visible_lines = area.height.saturating_sub(2) as usize; // 减去边框

    // 获取日志
    let all_logs = state.all_logs();
    let total_logs = all_logs.len();

    // 计算实际的滚动偏移（确保不超出范围）
    let max_offset = total_logs.saturating_sub(visible_lines);
    let actual_offset = (scroll_offset as usize).min(max_offset);

    // 获取要显示的日志
    let logs_to_show: Vec<_> = all_logs
        .iter()
        .skip(actual_offset)
        .take(visible_lines)
        .collect();

    // 构建日志行
    let mut lines = Vec::new();
    for log in logs_to_show {
        let time_str = log.timestamp.format("%H:%M:%S").to_string();
        let level_style = match log.level {
            LogLevel::Trace => Style::default().fg(Color::DarkGray),
            LogLevel::Debug => Style::default().fg(Color::Blue),
            LogLevel::Info => Style::default().fg(Color::Green),
            LogLevel::Warn => Style::default().fg(Color::Yellow),
            LogLevel::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", time_str),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("[{}] ", log.level.as_str()), level_style),
            Span::raw(&log.message),
        ]));
    }

    // 如果没有日志，显示提示
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "暂无日志...",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // 滚动指示器
    let scroll_indicator = if total_logs > visible_lines {
        format!(
            " [{}/{}] ↑↓ 滚动 | Home/End 跳转 ",
            actual_offset + 1,
            max_offset + 1
        )
    } else {
        String::new()
    };

    let log_viewer = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray))
                .title(format!(" 日志 ({} 条){} ", total_logs, scroll_indicator)),
        )
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);

    frame.render_widget(log_viewer, area);
}
