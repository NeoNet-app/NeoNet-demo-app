mod api;
mod app;
mod config;
mod ui;

use app::{ws_event_to_chat_message, App, ChatMessage};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::CrosstermBackend;
use std::io::stdout;

#[derive(Parser)]
#[command(name = "neonet-demo-chat", about = "NeoNet TUI chat demo")]
struct Cli {
    /// Daemon URL
    #[arg(long, default_value = "http://127.0.0.1:7780")]
    daemon: String,

    /// Join an existing room by ID
    #[arg(long)]
    room: Option<String>,

    /// Create a room and invite this NeoNet address
    #[arg(long)]
    invite: Option<String>,

    /// Temporary config: prompt pseudo interactively, persist nothing to disk
    #[arg(long)]
    tmpconf: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.room.is_none() && cli.invite.is_none() {
        eprintln!("Error: provide --room <id> or --invite <address>");
        std::process::exit(1);
    }

    // 1. Load token
    let token = config::load_token().map_err(|e| {
        eprintln!("{e}");
        e
    })?;

    let client = api::NeoNetClient::new(&cli.daemon, &token);

    // 2. Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;

    // 3. Get or ask pseudo
    let pseudo = if cli.tmpconf {
        ask_pseudo(&mut terminal)?
    } else {
        match config::load_config() {
            Some(cfg) => cfg.pseudo,
            None => {
                let p = ask_pseudo(&mut terminal)?;
                config::save_config(&config::AppConfig {
                    pseudo: p.clone(),
                })
                .ok();
                p
            }
        }
    };

    // 4. Fetch own address
    let own_address = client
        .get_identity()
        .await
        .map(|id| id.address)
        .unwrap_or_default();

    // 5. Determine room
    let room_id = if let Some(addr) = &cli.invite {
        let room_name = format!("Chat with {}", addr);
        let resp = client
            .create_room(&room_name, vec![addr.clone()])
            .await
            .map_err(|e| {
                cleanup_terminal();
                e
            })?;
        eprintln!("Room created: {}", resp.room_id);
        resp.room_id
    } else {
        cli.room.unwrap()
    };

    // 6. Connect WebSocket first (so we receive sync_status events)
    let mut app = App::new(pseudo.clone(), room_id.clone(), own_address);
    let mut ws_rx = None;
    let mut ws_tx = None;
    match client.connect_ws(&room_id).await {
        Ok((rx, tx)) => {
            ws_rx = Some(rx);
            ws_tx = Some(tx);
        }
        Err(e) => {
            app.add_message(ChatMessage {
                timestamp: "--:--".to_string(),
                display_name: "system".to_string(),
                text: format!("WebSocket failed: {e}"),
            });
        }
    }

    // 7. Try to load history — 404 means the room is still syncing
    match client.list_messages(&room_id).await {
        Ok(events) => {
            app.synced = true;
            app.load_history(&events);
        }
        Err(e) if e.contains("404") => {
            app.synced = false;
            app.add_message(ChatMessage {
                timestamp: "--:--".to_string(),
                display_name: "system".to_string(),
                text: "Room syncing, waiting for peers...".to_string(),
            });
        }
        Err(e) => {
            app.synced = true;
            app.add_message(ChatMessage {
                timestamp: "--:--".to_string(),
                display_name: "system".to_string(),
                text: format!("Failed to load history: {e}"),
            });
        }
    }

    // 8. Event loop
    let mut last_sync_retry = std::time::Instant::now();
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Check for WS messages (non-blocking), skip our own events
        if let Some(ref mut rx) = ws_rx {
            while let Ok(event) = rx.try_recv() {
                // Handle sync_status events
                if event.r#type == "sync_status" {
                    if let Some(ref room) = event.room_id {
                        if room == &app.room_id {
                            if event.status.as_deref() == Some("synced") && !app.synced {
                                // Room just synced — re-subscribe + load history
                                if let Some(ref tx) = ws_tx {
                                    let _ = tx.send(api::NeoNetClient::ws_subscribe_msg(&app.room_id));
                                }
                                app.synced = true;
                                app.messages.clear();
                                if let Ok(events) = client.list_messages(&app.room_id).await {
                                    app.load_history(&events);
                                }
                                app.add_message(ChatMessage {
                                    timestamp: "--:--".to_string(),
                                    display_name: "system".to_string(),
                                    text: "Room synced.".to_string(),
                                });
                            }
                        }
                    }
                    continue;
                }

                if event.author.as_deref() == Some(&app.own_address) {
                    continue;
                }
                if let Some(msg) = ws_event_to_chat_message(&event) {
                    app.add_message(msg);
                }
            }
        }

        // Periodic retry while waiting for sync (every 3 seconds)
        if !app.synced && last_sync_retry.elapsed() >= std::time::Duration::from_secs(3) {
            last_sync_retry = std::time::Instant::now();
            if let Ok(events) = client.list_messages(&room_id).await {
                // Room appeared — re-subscribe so the daemon pushes new events
                if let Some(ref tx) = ws_tx {
                    let _ = tx.send(api::NeoNetClient::ws_subscribe_msg(&room_id));
                }
                app.synced = true;
                app.messages.clear();
                app.load_history(&events);
                app.add_message(ChatMessage {
                    timestamp: "--:--".to_string(),
                    display_name: "system".to_string(),
                    text: "Room synced.".to_string(),
                });
            }
        }

        // Poll crossterm events with 50ms timeout
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
                {
                    app.should_quit = true;
                }
                match key.code {
                    KeyCode::Enter => {
                        if !app.input.is_empty() {
                            let text = app.input.drain(..).collect::<String>();
                            let ts = chrono::Local::now().format("%H:%M").to_string();
                            // Add locally immediately
                            app.add_message(ChatMessage {
                                timestamp: ts,
                                display_name: pseudo.clone(),
                                text: text.clone(),
                            });
                            // Send in background
                            let c = api::NeoNetClient::new(&cli.daemon, &token);
                            let rid = room_id.clone();
                            let dn = pseudo.clone();
                            tokio::spawn(async move {
                                let _ = c.send_message(&rid, &text, &dn).await;
                            });
                        }
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Char(c) => {
                        if !key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.input.push(c);
                        }
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    cleanup_terminal();
    Ok(())
}

fn cleanup_terminal() {
    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);
}

fn ask_pseudo(
    terminal: &mut ratatui::Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut input = String::new();
    loop {
        terminal.draw(|f| ui::draw_pseudo_input(f, &input))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
                {
                    cleanup_terminal();
                    std::process::exit(0);
                }
                match key.code {
                    KeyCode::Enter => {
                        let trimmed = input.trim().to_string();
                        if !trimmed.is_empty() {
                            return Ok(trimmed);
                        }
                    }
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                    }
                    _ => {}
                }
            }
        }
    }
}
