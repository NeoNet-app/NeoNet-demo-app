use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;

// ── Request / Response types ────────────────────────────────────────

#[derive(Serialize)]
struct CreateRoomRequest {
    name: String,
    room_type: String,
    members: Vec<String>,
}

#[derive(Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: String,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct DagEvent {
    pub event_id: String,
    pub author: Option<String>,
    pub kind: String,
    pub content: Value,
    pub ts_hint: i64,
    pub edited: bool,
    pub redacted: bool,
}

#[derive(Serialize)]
struct SendMessageRequest {
    text: String,
    display_name: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct SendMessageResponse {
    pub event_id: String,
    pub ts_hint: i64,
}

#[derive(Deserialize)]
struct MessagesResponse {
    messages: Vec<DagEvent>,
    #[allow(dead_code)]
    has_more: bool,
}

#[derive(Serialize)]
struct WsSubscribe {
    r#type: String,
    room_ids: Vec<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct WsEvent {
    pub r#type: String,
    pub room_id: Option<String>,
    pub event_id: Option<String>,
    pub author: Option<String>,
    pub kind: Option<String>,
    pub content: Option<Value>,
    pub ts_hint: Option<i64>,
    /// Present on sync_status events: "syncing" | "synced" | "error"
    pub status: Option<String>,
}

#[derive(Serialize)]
struct ConnectPeerRequest {
    addr: String,
}

#[derive(Deserialize)]
pub struct IdentityResponse {
    pub address: String,
}

// ── Client ──────────────────────────────────────────────────────────

pub struct NeoNetClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl NeoNetClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    /// Ask the daemon to open a QUIC+handshake connection to a peer or relay.
    /// Mirrors `neonet peers connect <addr>`.
    pub async fn connect_peer(&self, addr: &str) -> Result<(), String> {
        let url = format!("{}/v1/peers/connect", self.base_url);
        let body = ConnectPeerRequest {
            addr: addr.to_string(),
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Connect peer failed ({status}): {text}"));
        }
        Ok(())
    }

    pub async fn get_identity(&self) -> Result<IdentityResponse, String> {
        let url = format!("{}/v1/identity", self.base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Get identity failed ({status}): {text}"));
        }
        resp.json().await.map_err(|e| format!("Parse error: {e}"))
    }

    pub async fn create_room(
        &self,
        name: &str,
        members: Vec<String>,
    ) -> Result<CreateRoomResponse, String> {
        let url = format!("{}/v1/rooms", self.base_url);
        let body = CreateRoomRequest {
            name: name.to_string(),
            room_type: "direct".to_string(),
            members,
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Create room failed ({status}): {text}"));
        }
        resp.json().await.map_err(|e| format!("Parse error: {e}"))
    }

    pub async fn list_messages(&self, room_id: &str) -> Result<Vec<DagEvent>, String> {
        let url = format!("{}/v1/rooms/{}/messages?limit=200", self.base_url, room_id);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("List messages failed ({status}): {text}"));
        }
        let data: MessagesResponse = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        Ok(data.messages)
    }

    pub async fn send_message(
        &self,
        room_id: &str,
        text: &str,
        display_name: &str,
    ) -> Result<SendMessageResponse, String> {
        let url = format!("{}/v1/rooms/{}/messages", self.base_url, room_id);
        let body = SendMessageRequest {
            text: text.to_string(),
            display_name: display_name.to_string(),
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Send message failed ({status}): {text}"));
        }
        resp.json().await.map_err(|e| format!("Parse error: {e}"))
    }

    /// Connect WebSocket, subscribe to room, return a receiver for incoming events
    /// and a sender to send outgoing WS commands (e.g. re-subscribe after sync).
    pub async fn connect_ws(
        &self,
        room_id: &str,
    ) -> Result<(mpsc::UnboundedReceiver<WsEvent>, mpsc::UnboundedSender<String>), String> {
        let ws_url = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!("{ws_url}/v1/ws");

        // Extract host (with port) for the required Host header (RFC 6455)
        let host_header = ws_url
            .split("://")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .unwrap_or("localhost")
            .to_string();

        let request = tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Host", &host_header)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| format!("WS request build error: {e}"))?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| format!("WebSocket connect error: {e}"))?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to room
        let sub = WsSubscribe {
            r#type: "subscribe".to_string(),
            room_ids: vec![room_id.to_string()],
        };
        let sub_json = serde_json::to_string(&sub).unwrap();
        write
            .send(tungstenite::Message::Text(sub_json.into()))
            .await
            .map_err(|e| format!("WS send error: {e}"))?;

        let (tx, rx) = mpsc::unbounded_channel();

        // Channel for outgoing WS messages (re-subscribe, etc.)
        let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<String>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(tungstenite::Message::Text(text))) => {
                                if let Ok(event) = serde_json::from_str::<WsEvent>(&text) {
                                    if tx.send(event).is_err() {
                                        break;
                                    }
                                }
                            }
                            Some(Ok(tungstenite::Message::Close(_))) | Some(Err(_)) | None => break,
                            _ => {}
                        }
                    }
                    outgoing = ws_rx.recv() => {
                        match outgoing {
                            Some(text) => {
                                if write.send(tungstenite::Message::Text(text.into())).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        Ok((rx, ws_tx))
    }

    /// Send a WS subscribe message for a room (used to re-subscribe after sync).
    pub fn ws_subscribe_msg(room_id: &str) -> String {
        serde_json::to_string(&WsSubscribe {
            r#type: "subscribe".to_string(),
            room_ids: vec![room_id.to_string()],
        })
        .unwrap()
    }
}
