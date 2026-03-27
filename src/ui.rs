use crate::app::App;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Min(3),    // messages
        Constraint::Length(3), // input
    ])
    .split(frame.area());

    // ── Header ──────────────────────────────────────────────────────
    let header_text = format!(
        " NeoNet Demo Chat — {} — Room: {} ",
        app.pseudo, app.room_id
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD));
    frame.render_widget(header, chunks[0]);

    // ── Messages ────────────────────────────────────────────────────
    let msg_lines: Vec<Line> = app
        .messages
        .iter()
        .map(|m| {
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", m.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<10}", m.display_name),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" : {}", m.text)),
            ])
        })
        .collect();

    let visible_height = chunks[1].height.saturating_sub(2) as usize; // subtract borders
    let scroll = if msg_lines.len() > visible_height {
        (msg_lines.len() - visible_height) as u16
    } else {
        0
    };

    let messages = Paragraph::new(msg_lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(messages, chunks[1]);

    // ── Input ───────────────────────────────────────────────────────
    let input_text = format!("> {}", app.input);
    let input = Paragraph::new(input_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(input, chunks[2]);

    // Place cursor after input text
    let cursor_x = chunks[2].x + 1 + 2 + app.input.len() as u16; // border + "> " + text
    let cursor_y = chunks[2].y + 1; // border
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Draw the pseudo input screen (shown before chat if no config exists).
pub fn draw_pseudo_input(frame: &mut Frame, input: &str) {
    let chunks = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Length(3),
        Constraint::Percentage(40),
    ])
    .split(frame.area());

    let prompt = Paragraph::new(format!("  Pseudo : {}", input))
        .block(
            Block::default()
                .title(" NeoNet Demo Chat — Choisis ton pseudo ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White));
    frame.render_widget(prompt, chunks[1]);

    let cursor_x = chunks[1].x + 1 + 10 + input.len() as u16;
    let cursor_y = chunks[1].y + 1;
    frame.set_cursor_position((cursor_x, cursor_y));
}
