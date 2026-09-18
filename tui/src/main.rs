use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::{io, time::Duration};

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = run_ui(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

fn run_ui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    loop {
        terminal.draw(|frame| {
            let area = frame.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);

            let header = Paragraph::new("RALPS TUI — press q to quit")
                .block(Block::default().borders(Borders::ALL).title("Header"))
                .style(Style::default().fg(Color::Cyan));
            frame.render_widget(header, chunks[0]);

            let body = Paragraph::new("Interactive dashboard scaffold is ready.")
                .block(Block::default().borders(Borders::ALL).title("Status"));
            frame.render_widget(body, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    Ok(())
}
