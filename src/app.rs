use crate::api::{DagEvent, WsEvent};
use chrono::{Local, TimeZone};

#[derive(Clone)]
pub struct ChatMessage {
    pub timestamp: String,
    pub display_name: String,
    pub text: String,
}

pub struct App {
    pub pseudo: String,
    pub room_id: String,
    pub own_address: String,
    pub synced: bool,
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(pseudo: String, room_id: String, own_address: String) -> Self {
        Self {
            pseudo,
            room_id,
            own_address,
            synced: false,
            messages: Vec::new(),
            input: String::new(),
            should_quit: false,
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    /// Convert DagEvent list (from history) into ChatMessages.
    pub fn load_history(&mut self, events: &[DagEvent]) {
        for event in events {
            if event.kind != "message" || event.redacted {
                continue;
            }
            if let Some(msg) = dag_event_to_chat_message(event) {
                self.messages.push(msg);
            }
        }
    }
}

pub fn dag_event_to_chat_message(event: &DagEvent) -> Option<ChatMessage> {
    let text = event.content.get("text")?.as_str()?.to_string();
    let display_name = event
        .content
        .get("display_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            event
                .author
                .as_deref()
                .map(|a| a.chars().take(8).collect())
        })
        .unwrap_or_else(|| "???".to_string());
    let timestamp = format_ts(event.ts_hint);
    Some(ChatMessage {
        timestamp,
        display_name,
        text,
    })
}

pub fn ws_event_to_chat_message(event: &WsEvent) -> Option<ChatMessage> {
    if event.r#type != "new_message" {
        return None;
    }
    let content = event.content.as_ref()?;
    let text = content.get("text")?.as_str()?.to_string();
    let display_name = content
        .get("display_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            event
                .author
                .as_deref()
                .map(|a| a.chars().take(8).collect())
        })
        .unwrap_or_else(|| "???".to_string());
    let timestamp = format_ts(event.ts_hint.unwrap_or(0));
    Some(ChatMessage {
        timestamp,
        display_name,
        text,
    })
}

fn format_ts(unix_secs: i64) -> String {
    Local
        .timestamp_opt(unix_secs, 0)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_string())
}
