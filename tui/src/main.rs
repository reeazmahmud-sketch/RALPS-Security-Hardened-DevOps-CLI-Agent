use anyhow::Result;
use api::AutopilotService;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::{io, time::Duration};

#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = run_ui(&mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

async fn run_ui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let autopilot = AutopilotService::default();

    loop {
        let state = autopilot.status().await?;
        let schedules = autopilot.list_schedules().await?;

        terminal.draw(|frame| {
            let area = frame.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(5),
                    Constraint::Min(8),
                    Constraint::Min(8),
                ])
                .split(area);

            let header = Paragraph::new("RALPS TUI — q quits, r refreshes")
                .block(Block::default().borders(Borders::ALL).title("Header"))
                .style(Style::default().fg(Color::Cyan));
            frame.render_widget(header, chunks[0]);

            let status_text = vec![
                Line::from(vec![
                    Span::raw("Autopilot: "),
                    Span::styled(
                        if state.running { "running" } else { "stopped" },
                        Style::default().fg(if state.running { Color::Green } else { Color::Yellow }),
                    ),
                ]),
                Line::from(format!("Schedules: {}", state.schedule_count)),
                Line::from(format!(
                    "Last tick: {}",
                    state
                        .last_tick_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "never".to_string())
                )),
            ];
            let status = Paragraph::new(status_text)
                .block(Block::default().borders(Borders::ALL).title("Status"));
            frame.render_widget(status, chunks[1]);

            let schedule_items: Vec<ListItem> = if schedules.schedules.is_empty() {
                vec![ListItem::new("No schedules configured in ~/.ralps/autopilot.toml")]
            } else {
                schedules
                    .schedules
                    .iter()
                    .map(|schedule| {
                        let mode = schedule.command.as_deref().unwrap_or(schedule.prompt.as_str());
                        ListItem::new(format!(
                            "{} [{}] {}",
                            schedule.name,
                            schedule.cron,
                            truncate(mode, 60)
                        ))
                    })
                    .collect()
            };
            let schedules_list = List::new(schedule_items)
                .block(Block::default().borders(Borders::ALL).title("Schedules"));
            frame.render_widget(schedules_list, chunks[2]);

            let run_items: Vec<ListItem> = if state.recent_runs.is_empty() {
                vec![ListItem::new("No recent runs recorded")]
            } else {
                state
                    .recent_runs
                    .iter()
                    .rev()
                    .map(|run| {
                        ListItem::new(format!(
                            "{} [{}] {}",
                            run.schedule_name,
                            if run.success { "ok" } else { "failed" },
                            truncate(&run.summary, 70)
                        ))
                    })
                    .collect()
            };
            let runs = List::new(run_items)
                .block(Block::default().borders(Borders::ALL).title("Recent Runs"));
            frame.render_widget(runs, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => continue,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        value.to_string()
    } else {
        format!("{}...", &value[..max_len])
    }
}
