use crate::events::{RunEvent, TokenUsage};
use crate::models::{Conversation, Message, Role};
use crate::orchestrator::Orchestrator;
use crate::storage::Storage;
use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style, Stylize},
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
    cursor: usize,
    messages: Vec<Message>,
    conversations: Vec<Conversation>,
    current_conv_id: Option<Uuid>,
    is_loading: bool,
    active_run: Option<Uuid>,
    cancellation: Option<CancellationToken>,
    event_tx: UnboundedSender<RunEvent>,
    event_rx: UnboundedReceiver<RunEvent>,
    trace_log: Vec<String>,
    token_usage: TokenUsage,
    stage: Option<(String, usize, usize)>,
    error_message: Option<String>,
    error_retryable: bool,
    last_query: Option<String>,
    last_failed_query: Option<String>,
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
            cursor: 0,
            messages,
            conversations,
            current_conv_id,
            is_loading: false,
            active_run: None,
            cancellation: None,
            event_tx,
            event_rx,
            trace_log: Vec::new(),
            token_usage: TokenUsage::default(),
            stage: None,
            error_message: None,
            error_retryable: false,
            last_query: None,
            last_failed_query: None,
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
        self.cursor = 0;
        self.last_query = Some(query.clone());
        self.messages.push(Message {
            id: Uuid::new_v4(),
            conversation_id: conv_id,
            role: Role::User,
            content: query.clone(),
            created_at: Utc::now(),
            thinking: None,
        });
        self.error_message = None;
        self.error_retryable = false;
        self.token_usage = TokenUsage::default();
        self.stage = None;
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
                RunEvent::Usage { run_id, usage } => {
                    if self.active_run == Some(run_id) {
                        self.token_usage.add_assign(&usage);
                    }
                }
                RunEvent::Stage {
                    run_id,
                    agent,
                    current,
                    total,
                } => {
                    if self.active_run == Some(run_id) {
                        self.stage = Some((agent, current, total));
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
                        self.stage = None;
                        self.last_failed_query = None;
                    }
                }
                RunEvent::Failed {
                    run_id,
                    conversation_id,
                    message,
                    retryable,
                } => {
                    if self.active_run == Some(run_id)
                        && self.current_conv_id == Some(conversation_id)
                    {
                        self.is_loading = false;
                        self.active_run = None;
                        self.cancellation = None;
                        self.stage = None;
                        self.error_message = Some(message);
                        self.error_retryable = retryable;
                        self.last_failed_query = self.last_query.clone();
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
        self.stage = None;
    }

    pub fn insert_char(&mut self, character: char) {
        self.input.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    pub fn delete_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.input[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.input.drain(previous..self.cursor);
        self.cursor = previous;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let next = self.input[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.input.len());
        self.input.drain(self.cursor..next);
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += self.input[self.cursor..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0);
        }
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    pub async fn retry_last_query(&mut self) -> Result<()> {
        let Some(query) = self.last_failed_query.clone() else {
            return Ok(());
        };
        self.input = query;
        self.cursor = self.input.len();
        self.send_message().await
    }

    pub fn dismiss_error(&mut self) {
        self.error_message = None;
        self.error_retryable = false;
        self.last_failed_query = None;
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
    let mut app = App::new(orchestrator, storage).await?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let loop_result = run_loop(&mut terminal, &mut app).await;
    let restore_result = restore_terminal(&mut terminal);
    loop_result?;
    restore_result?;
    Ok(())
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        app.process_events();
        app.refresh_messages().await?;
        terminal.draw(|f| ui(f, app))?;

        tokio::select! {
            maybe_event = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe_event
                    && handle_key_event(app, key).await?
                {
                    break;
                }
            }
            _ = tick.tick() => {}
        }
    }
    Ok(())
}

async fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool> {
    if key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        app.cancel_active_run();
        return Ok(true);
    }

    if app.error_message.is_some() {
        match key.code {
            KeyCode::Char('r') if app.error_retryable => app.retry_last_query().await?,
            KeyCode::Char('d') => app.dismiss_error(),
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Enter => app.send_message().await?,
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'u' => {
            app.clear_input();
        }
        KeyCode::Char(c) => app.insert_char(c),
        KeyCode::Backspace => app.delete_backward(),
        KeyCode::Delete => app.delete_forward(),
        KeyCode::Left => app.move_cursor_left(),
        KeyCode::Right => app.move_cursor_right(),
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input.len(),
        KeyCode::Tab => app.next_conv().await?,
        KeyCode::F(2) => {
            app.cancel_active_run();
            let conv = app.storage.create_conversation("Nuevo Chat").await?;
            app.current_conv_id = Some(conv.id);
            app.conversations = app.storage.list_conversations().await?;
            app.messages = Vec::new();
            app.trace_log.clear();
            app.dismiss_error();
        }
        KeyCode::F(4) => {
            app.cancel_active_run();
            if let Some(id) = app.current_conv_id {
                app.storage.delete_conversation(id).await?;
                app.conversations = app.storage.list_conversations().await?;
                app.current_conv_id = app.conversations.first().map(|c| c.id);
                app.messages = if let Some(new_id) = app.current_conv_id {
                    app.storage.get_messages(new_id).await?
                } else {
                    Vec::new()
                };
                app.trace_log.clear();
            }
        }
        KeyCode::Up => app.scroll_offset = app.scroll_offset.saturating_add(1),
        KeyCode::Down => app.scroll_offset = app.scroll_offset.saturating_sub(1),
        KeyCode::PageUp => app.scroll_offset = app.scroll_offset.saturating_add(10),
        KeyCode::PageDown => app.scroll_offset = app.scroll_offset.saturating_sub(10),
        KeyCode::F(6) => {
            app.cancel_active_run();
            app.storage.delete_profile().await?;
            app.messages.push(Message {
                id: Uuid::new_v4(),
                conversation_id: app.current_conv_id.unwrap_or(Uuid::nil()),
                role: Role::System,
                content: "⚠️ Perfil reiniciado. En la próxima consulta se volverá a pedir información si es necesario.".to_string(),
                created_at: Utc::now(),
                thinking: None,
            });
        }
        _ => {}
    }
    Ok(false)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
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
    let error_height = if app.error_message.is_some() { 3 } else { 1 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(error_height),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    let title = app
        .current_conv_id
        .and_then(|id| {
            app.conversations
                .iter()
                .find(|conversation| conversation.id == id)
        })
        .map(|conversation| conversation.title.as_str())
        .unwrap_or("Stock Agent");
    let status = if let Some((agent, current, total)) = &app.stage {
        format!("{agent} {current}/{total}")
    } else if app.is_loading {
        "Pensando…".to_string()
    } else {
        "Listo".to_string()
    };
    let usage = format!(
        "tokens in: {} • out: {} • total: {}",
        app.token_usage.prompt_tokens,
        app.token_usage.completion_tokens,
        app.token_usage.total()
    );
    let header_line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(format!(" {} ", title), Style::default().bold().cyan()),
        ratatui::text::Span::raw("  "),
        ratatui::text::Span::styled(status, Style::default().bold().yellow()),
        ratatui::text::Span::raw("  "),
        ratatui::text::Span::styled(usage, Style::default().dim()),
    ]);
    let header = Paragraph::new(header_line)
        .block(Block::default().borders(Borders::ALL).title("Conversación"));
    f.render_widget(header, chunks[0]);

    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(chunks[1]);

    let mut message_lines = Vec::new();
    for message in &app.messages {
        let (label, style) = match message.role {
            Role::User => ("❯ Tú", Style::default().bold().cyan()),
            Role::Assistant => ("◆ Agente", Style::default().bold().green()),
            _ => ("⚠ Sistema", Style::default().bold().yellow()),
        };
        message_lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            label, style,
        )));
        message_lines.extend(format_markdown(&message.content));
        message_lines.push(ratatui::text::Line::from(""));
    }
    if app.is_loading {
        message_lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled("◆ Agente ", Style::default().bold().green()),
            ratatui::text::Span::styled("está trabajando…", Style::default().dim()),
        ]));
    }

    let view_height = middle_chunks[0].height.saturating_sub(2);
    let inner_width = middle_chunks[0].width.saturating_sub(2).max(1);
    let wrapped_lines = wrapped_line_count(&message_lines, inner_width);
    let max_scroll = wrapped_lines.saturating_sub(view_height);
    // scroll_offset is the distance from the bottom: zero always means latest.
    let scroll = max_scroll.saturating_sub(app.scroll_offset);
    let messages_widget = Paragraph::new(message_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Chat · ↑/↓ desplazar"),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(messages_widget, middle_chunks[0]);

    let trace_lines = if app.trace_log.is_empty() {
        vec![ratatui::text::Line::from("Esperando eventos…".dim())]
    } else {
        app.trace_log
            .iter()
            .rev()
            .map(|log| ratatui::text::Line::from(log.clone().dim()))
            .collect::<Vec<_>>()
    };
    let trace_widget = Paragraph::new(trace_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Progreso · UTC"),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(trace_widget, middle_chunks[1]);

    if let Some(error) = &app.error_message {
        let action = if app.error_retryable {
            "r reintentar · d descartar"
        } else {
            "d descartar"
        };
        let error_widget = Paragraph::new(vec![
            ratatui::text::Line::from(ratatui::text::Span::styled(
                format!("⚠ Error: {error}"),
                Style::default().bold().red(),
            )),
            ratatui::text::Line::from(ratatui::text::Span::styled(
                action,
                Style::default().yellow(),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("La consulta falló"),
        );
        f.render_widget(error_widget, chunks[2]);
    } else {
        f.render_widget(Paragraph::new(""), chunks[2]);
    }

    let input_label = if app.is_loading {
        "⏳ Procesando · Esc cancela"
    } else {
        "⌨ Entrada · Enter enviar · Ctrl+U limpiar"
    };
    let input_area = chunks[3];
    let inner_width = input_area.width.saturating_sub(2) as usize;
    let horizontal_offset = input_horizontal_offset(app, inner_width);
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().cyan())
        .block(Block::default().borders(Borders::ALL).title(input_label))
        .scroll((0, horizontal_offset as u16));
    f.render_widget(input, input_area);
    if inner_width > 0 {
        let cursor_column = app.input[..app.cursor].chars().count();
        let visible_column = cursor_column.saturating_sub(horizontal_offset);
        let cursor_x = input_area.x + 1 + visible_column.min(inner_width.saturating_sub(1)) as u16;
        f.set_cursor_position((cursor_x, input_area.y + 1));
    }

    let help = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(" Tab ", Style::default().bold().cyan()),
        ratatui::text::Span::styled("chat siguiente  ", Style::default().dim()),
        ratatui::text::Span::styled("F2", Style::default().bold().cyan()),
        ratatui::text::Span::styled(" nuevo  ", Style::default().dim()),
        ratatui::text::Span::styled("F4", Style::default().bold().cyan()),
        ratatui::text::Span::styled(" borrar  ", Style::default().dim()),
        ratatui::text::Span::styled("Esc", Style::default().bold().cyan()),
        ratatui::text::Span::styled(" salir", Style::default().dim()),
    ]);
    f.render_widget(Paragraph::new(help), chunks[4]);
}

fn input_horizontal_offset(app: &App, width: usize) -> usize {
    if width == 0 {
        return 0;
    }
    let cursor_column = app.input[..app.cursor].chars().count();
    cursor_column.saturating_sub(width.saturating_sub(1))
}

fn wrapped_line_count(lines: &[ratatui::text::Line<'_>], width: u16) -> u16 {
    let width = usize::from(width.max(1));
    lines
        .iter()
        .map(|line| {
            let characters = line.to_string().chars().count();
            characters.max(1).div_ceil(width) as u16
        })
        .fold(0, u16::saturating_add)
}
