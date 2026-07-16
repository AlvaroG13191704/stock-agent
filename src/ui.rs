use crate::events::RunEvent;
use crate::models::{Conversation, Message, Role};
use crate::orchestrator::Orchestrator;
use crate::storage::Storage;
use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct App {
    orchestrator: Arc<Orchestrator>,
    storage: Arc<Storage>,
    input: String,
    messages: Vec<Message>,
    conversations: Vec<Conversation>,
    current_conv_id: Option<Uuid>,
    is_loading: bool,
    active_run: Option<Uuid>,
    cancellation: Option<CancellationToken>,
    event_tx: UnboundedSender<RunEvent>,
    event_rx: UnboundedReceiver<RunEvent>,
    trace_log: Vec<String>,
    pub scroll_offset: u16,
}

impl App {
    pub async fn new(orchestrator: Arc<Orchestrator>, storage: Arc<Storage>) -> Result<Self> {
        let conversations = storage.list_conversations().await?;
        let current_conv_id = conversations.first().map(|c| c.id);
        let messages = if let Some(id) = current_conv_id {
            storage.get_messages(id).await?
        } else {
            Vec::new()
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            orchestrator,
            storage,
            input: String::new(),
            messages,
            conversations,
            current_conv_id,
            is_loading: false,
            active_run: None,
            cancellation: None,
            event_tx,
            event_rx,
            trace_log: Vec::new(),
            scroll_offset: 0,
        })
    }

    pub async fn send_message(&mut self) -> Result<()> {
        if self.input.trim().is_empty() || self.is_loading {
            return Ok(());
        }

        let conv_id = if let Some(id) = self.current_conv_id {
            id
        } else {
            let conv = self.storage.create_conversation("Nuevo Chat").await?;
            self.current_conv_id = Some(conv.id);
            self.conversations = self.storage.list_conversations().await?;
            conv.id
        };

        let query = self.input.trim().to_string();
        self.input.clear();
        self.is_loading = true;
        self.trace_log.clear();

        let run_id = Uuid::new_v4();
        let cancellation = CancellationToken::new();
        self.active_run = Some(run_id);
        self.cancellation = Some(cancellation.clone());

        let orchestrator = self.orchestrator.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let _ = orchestrator
                .handle_query(conv_id, query, run_id, cancellation, event_tx)
                .await;
        });

        self.scroll_offset = 0;
        Ok(())
    }

    pub fn process_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                RunEvent::Started {
                    run_id,
                    conversation_id,
                } => {
                    if self.active_run == Some(run_id)
                        && self.current_conv_id == Some(conversation_id)
                    {
                        self.is_loading = true;
                    }
                }
                RunEvent::Trace { run_id, message } => {
                    if self.active_run == Some(run_id) {
                        self.trace_log.push(message);
                        if self.trace_log.len() > 30 {
                            self.trace_log.remove(0);
                        }
                    }
                }
                RunEvent::Completed {
                    run_id,
                    conversation_id,
                } => {
                    if self.active_run == Some(run_id)
                        && self.current_conv_id == Some(conversation_id)
                    {
                        self.is_loading = false;
                        self.active_run = None;
                        self.cancellation = None;
                    }
                }
                RunEvent::Failed {
                    run_id,
                    conversation_id,
                    message,
                } => {
                    if self.active_run == Some(run_id) {
                        self.is_loading = false;
                        self.active_run = None;
                        self.cancellation = None;
                        self.messages.push(Message {
                            id: Uuid::new_v4(),
                            conversation_id,
                            role: Role::System,
                            content: format!("⚠️ No se pudo completar la consulta: {message}"),
                            created_at: Utc::now(),
                            thinking: None,
                        });
                    }
                }
            }
        }
    }

    pub fn cancel_active_run(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.active_run = None;
        self.is_loading = false;
    }

    pub async fn refresh_messages(&mut self) -> Result<()> {
        if let Some(id) = self.current_conv_id {
            let new_messages = self.storage.get_messages(id).await?;
            if new_messages.len() > self.messages.len() {
                self.messages = new_messages;
            }
        }
        Ok(())
    }

    pub async fn next_conv(&mut self) -> Result<()> {
        if self.conversations.is_empty() {
            return Ok(());
        }
        self.cancel_active_run();
        let idx = self
            .conversations
            .iter()
            .position(|c| Some(c.id) == self.current_conv_id)
            .unwrap_or(0);
        let next_idx = (idx + 1) % self.conversations.len();
        let next_id = self.conversations[next_idx].id;
        self.current_conv_id = Some(next_id);
        self.messages = self.storage.get_messages(next_id).await?;
        self.trace_log.clear();
        Ok(())
    }
}

pub async fn run(orchestrator: Arc<Orchestrator>, storage: Arc<Storage>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(orchestrator, storage).await?;

    loop {
        app.process_events();
        app.refresh_messages().await?;
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(std::time::Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Esc => {
                    app.cancel_active_run();
                    break;
                }
                KeyCode::Enter => {
                    app.send_message().await?;
                }
                KeyCode::Char(c) => {
                    app.input.push(c);
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Tab => {
                    app.next_conv().await?;
                }
                KeyCode::F(2) => {
                    app.cancel_active_run();
                    let conv = app.storage.create_conversation("Nuevo Chat").await?;
                    app.current_conv_id = Some(conv.id);
                    app.conversations = app.storage.list_conversations().await?;
                    app.messages = Vec::new();
                    app.trace_log.clear();
                }
                KeyCode::Delete | KeyCode::F(4) => {
                    app.cancel_active_run();
                    if let Some(id) = app.current_conv_id {
                        app.storage.delete_conversation(id).await?;
                        app.conversations = app.storage.list_conversations().await?;
                        app.current_conv_id = app.conversations.first().map(|c| c.id);
                        if let Some(new_id) = app.current_conv_id {
                            app.messages = app.storage.get_messages(new_id).await?;
                        } else {
                            app.messages = Vec::new();
                        }
                        app.trace_log.clear();
                    }
                }
                KeyCode::Up => {
                    if app.scroll_offset > 0 {
                        app.scroll_offset -= 1;
                    }
                }
                KeyCode::Down => {
                    app.scroll_offset += 1;
                }
                KeyCode::PageUp => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    app.scroll_offset = app.scroll_offset.saturating_add(10);
                }
                KeyCode::F(6) => {
                    app.cancel_active_run();
                    app.storage.delete_profile().await?;
                    app.messages.push(Message {
                            id: Uuid::new_v4(),
                            conversation_id: app.current_conv_id.unwrap_or(Uuid::nil()),
                            role: Role::System,
                            content: "⚠️ Perfil Reiniciado. En la próxima consulta se volverá a pedir información si es necesario.".to_string(),
                            created_at: Utc::now(),
                            thinking: None,
                        });
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn format_markdown(text: &str) -> Vec<ratatui::text::Line<'_>> {
    let mut lines = Vec::new();
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        let line = raw_lines[i];

        if line.starts_with("|") && i + 1 < raw_lines.len() && raw_lines[i + 1].contains("---") {
            // Table detected
            let mut table_rows = Vec::new();
            let mut j = i;
            while j < raw_lines.len() && raw_lines[j].starts_with("|") {
                table_rows.push(raw_lines[j]);
                j += 1;
            }

            if table_rows.len() >= 3 {
                // We have a header, separator, and at least one data row
                let rendered = render_premium_table(&table_rows);
                lines.extend(rendered);
                i = j;
                continue;
            }
        }

        if line.starts_with("#") {
            // Header
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled(
                    line,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else if line.contains("**") {
            // Simple Bold parser
            let parts: Vec<&str> = line.split("**").collect();
            let mut spans = Vec::new();
            for (idx, part) in parts.into_iter().enumerate() {
                if idx % 2 == 1 {
                    spans.push(ratatui::text::Span::styled(
                        part,
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::Cyan),
                    ));
                } else {
                    spans.push(ratatui::text::Span::raw(part));
                }
            }
            lines.push(ratatui::text::Line::from(spans));
        } else {
            lines.push(ratatui::text::Line::raw(line));
        }
        i += 1;
    }
    lines
}

fn render_premium_table(raw_rows: &[&str]) -> Vec<ratatui::text::Line<'static>> {
    let mut grid: Vec<Vec<String>> = Vec::new();
    for row in raw_rows {
        if row.contains("---") {
            continue;
        } // Skip separator
        let cols: Vec<String> = row
            .split('|')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect();
        if !cols.is_empty() {
            grid.push(cols);
        }
    }

    if grid.is_empty() {
        return vec![];
    }

    let num_cols = grid[0].len();
    let mut col_widths = vec![0; num_cols];
    for row in &grid {
        for (idx, col) in row.iter().enumerate() {
            if idx < num_cols {
                col_widths[idx] = col_widths[idx].max(col.len());
            }
        }
    }

    let mut result = Vec::new();

    // Top border
    let mut top = String::from("┌");
    for (idx, &w) in col_widths.iter().enumerate() {
        top.push_str(&"─".repeat(w + 2));
        if idx < num_cols - 1 {
            top.push('┬');
        } else {
            top.push('┐');
        }
    }
    result.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        top,
        Style::default().fg(Color::DarkGray),
    )));

    for (row_idx, row) in grid.iter().enumerate() {
        let mut line_content = String::from("│");
        for (col_idx, &w) in col_widths.iter().enumerate() {
            let val = row.get(col_idx).cloned().unwrap_or_default();
            line_content.push_str(&format!(" {:<width$} │", val, width = w));
        }

        let style = if row_idx == 0 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        result.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            line_content,
            style,
        )));

        // Separator after header
        if row_idx == 0 {
            let mut sep = String::from("├");
            for (idx, &w) in col_widths.iter().enumerate() {
                sep.push_str(&"─".repeat(w + 2));
                if idx < num_cols - 1 {
                    sep.push('┼');
                } else {
                    sep.push('┤');
                }
            }
            result.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                sep,
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    // Bottom border
    let mut bottom = String::from("└");
    for (idx, &w) in col_widths.iter().enumerate() {
        bottom.push_str(&"─".repeat(w + 2));
        if idx < num_cols - 1 {
            bottom.push('┴');
        } else {
            bottom.push('┘');
        }
    }
    result.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        bottom,
        Style::default().fg(Color::DarkGray),
    )));

    result
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.area());

    let title = if let Some(id) = app.current_conv_id {
        app.conversations
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.as_str())
            .unwrap_or("Stock Agent")
    } else {
        "Stock Agent"
    };

    let header = Paragraph::new(format!(
        "Chat: {} (Tab Switch, F2 Nuevo, F4 Borrar, F6 Reset Perfil, ESC Salir)",
        title
    ))
    .style(Style::default().fg(Color::Yellow))
    .block(Block::default().borders(Borders::ALL).title("Conversación"));
    f.render_widget(header, chunks[0]);

    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)].as_ref())
        .split(chunks[1]);

    let mut messages_spans = Vec::new();
    for m in &app.messages {
        let role_label = match m.role {
            Role::User => "Tú",
            Role::Assistant => "Agente",
            _ => "Sistema",
        };
        let color = match m.role {
            Role::User => Color::Cyan,
            Role::Assistant => Color::Green,
            _ => Color::DarkGray,
        };

        messages_spans.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(
                format!("{}: ", role_label),
                Style::default().add_modifier(Modifier::BOLD).fg(color),
            ),
        ]));

        // Render Markdown content
        let content_lines = format_markdown(&m.content);
        for line in content_lines {
            messages_spans.push(line);
        }
        messages_spans.push(ratatui::text::Line::from(""));
    }

    // Auto-scroll logic:
    // User can scroll with Up/Down/PgUp/PgDown.
    // If scroll_offset is 0, we stick to the bottom.
    let total_lines = messages_spans.len() as u16;
    let view_height = middle_chunks[0].height;

    let scroll = if app.scroll_offset == 0 {
        total_lines.saturating_sub(view_height.saturating_sub(2))
    } else {
        app.scroll_offset
    };

    let messages_widget = Paragraph::new(messages_spans)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Mensajes (↑/↓ para Scroll, Esc para salir)"),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(messages_widget, middle_chunks[0]);

    // Trace log sidebar (UTC)
    let trace_spans = app
        .trace_log
        .iter()
        .rev()
        .map(|log| ratatui::text::Line::from(log.clone()))
        .collect::<Vec<_>>();

    let trace_widget = Paragraph::new(trace_spans)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Trazabilidad (UTC)"),
        )
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });
    f.render_widget(trace_widget, middle_chunks[1]);

    let input_label = if app.is_loading {
        "⏳ Procesando..."
    } else {
        "⌨️ Entrada (Enter para enviar)"
    };
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(input_label));
    f.render_widget(input, chunks[2]);
}
