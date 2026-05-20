use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::events::{IpcAction, IpcEvent};

pub const IPC_PORT: u16 = 9712;
pub const IPC_ADDR: &str = "127.0.0.1";

/// How often the server sends a WebSocket Ping to each client.
const CLIENT_PING_INTERVAL_S: u64 = 30;
/// How long without any message from a client before the connection is closed.
const CLIENT_IDLE_TIMEOUT_S: u64 = 300;

/// Global monotonic sequence counter.  Incremented on every outgoing event.
static IPC_SEQ: AtomicU64 = AtomicU64::new(0);
/// Per-session counter.  Incremented each time a new client connects.
/// The frontend can use this to detect reconnects and resync state.
static IPC_SESSION: AtomicU64 = AtomicU64::new(0);

static BROADCAST_TX: OnceCell<broadcast::Sender<IpcEvent>> = OnceCell::new();
static ACTION_HANDLER: OnceCell<Arc<RwLock<Option<Box<dyn Fn(IpcAction) + Send + Sync>>>>> = OnceCell::new();
static AUTH_TOKEN: OnceCell<String> = OnceCell::new();
static SANDBOX_WARNINGS: OnceCell<Vec<String>> = OnceCell::new();

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Wrap an IpcEvent in its wire envelope: { seq, session, ts, ...event }.
/// Returns a JSON string ready for transmission.
fn wrap_event(event: &IpcEvent) -> Result<String, serde_json::Error> {
    let seq = IPC_SEQ.fetch_add(1, Ordering::Relaxed);
    let session = IPC_SESSION.load(Ordering::Relaxed);
    let ts = now_ms();

    // Serialize the event to a Value, then inject envelope fields.
    let mut obj = match serde_json::to_value(event)? {
        serde_json::Value::Object(m) => m,
        v => {
            let mut m = serde_json::Map::new();
            m.insert("data".to_string(), v);
            m
        }
    };
    obj.insert("seq".to_string(), serde_json::Value::Number(seq.into()));
    obj.insert("session".to_string(), serde_json::Value::Number(session.into()));
    obj.insert("ts".to_string(), serde_json::Value::Number(ts.into()));
    serde_json::to_string(&serde_json::Value::Object(obj))
}

/// Return the current IPC sequence number (last assigned seq, not next).
pub fn current_seq() -> u64 {
    IPC_SEQ.load(Ordering::Relaxed).saturating_sub(1)
}

/// Return the current IPC session number.
pub fn current_session() -> u64 {
    IPC_SESSION.load(Ordering::Relaxed)
}

pub fn set_auth_token(token: String) {
    AUTH_TOKEN.set(token).ok();
}

pub fn set_sandbox_warnings(ids: Vec<String>) {
    SANDBOX_WARNINGS.set(ids).ok();
}

// Initialize the IPC broadcast channel
pub fn init() -> broadcast::Sender<IpcEvent> {
    if let Some(tx) = BROADCAST_TX.get() {
        return tx.clone();
    }

    let (tx, _) = broadcast::channel::<IpcEvent>(256);
    BROADCAST_TX.set(tx.clone()).ok();
    ACTION_HANDLER.set(Arc::new(RwLock::new(None))).ok();
    
    info!("IPC: Broadcast channel initialized");
    tx
}

// Send event to all connected clients
pub fn send(event: IpcEvent) {
    if let Some(tx) = BROADCAST_TX.get() {
        match tx.send(event.clone()) {
            Ok(n) => {
                if n > 0 {
                    debug!("IPC: Sent {:?} to {} client(s)", event, n);
                }
            }
            Err(_) => {
                // no receivers, that's fine
            }
        }
    }
}

// Register handler for incoming actions from GUI
pub fn set_action_handler<F>(handler: F)
where
    F: Fn(IpcAction) + Send + Sync + 'static,
{
    if let Some(h) = ACTION_HANDLER.get() {
        *h.write() = Some(Box::new(handler));
    }
}

fn handle_action(action: IpcAction) {
    info!("IPC: Received action {:?}", action);
    
    // handle ping internally
    if matches!(action, IpcAction::Ping) {
        send(IpcEvent::Pong);
        return;
    }
    
    // forward to registered handler
    if let Some(handler_lock) = ACTION_HANDLER.get() {
        let handler = handler_lock.read();
        if let Some(ref h) = *handler {
            h(action);
        }
    }
}

// Start the WebSocket server (blocking)
pub async fn start_server() {
    let addr = format!("{}:{}", IPC_ADDR, IPC_PORT);
    let socket_addr: SocketAddr = addr.parse().expect("Invalid IPC address");

    let listener = match TcpListener::bind(&socket_addr).await {
        Ok(l) => {
            info!("IPC: WebSocket server listening on ws://{}", addr);
            l
        }
        Err(e) => {
            error!("IPC: Failed to bind to {}: {}", addr, e);
            return;
        }
    };

    // notify that we're ready
    send(IpcEvent::Started);

    while let Ok((stream, peer_addr)) = listener.accept().await {
        info!("IPC: Client connecting from {}", peer_addr);
        
        let rx = BROADCAST_TX
            .get()
            .map(|tx| tx.subscribe())
            .expect("IPC not initialized");

        tokio::spawn(handle_client(stream, peer_addr, rx));
    }
}

async fn handle_client(
    stream: TcpStream,
    peer_addr: SocketAddr,
    mut event_rx: broadcast::Receiver<IpcEvent>,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => {
            info!("IPC: Client connected: {}", peer_addr);
            ws
        }
        Err(e) => {
            error!("IPC: WebSocket handshake failed for {}: {}", peer_addr, e);
            return;
        }
    };

    // Assign a session ID to this connection.
    let conn_session = IPC_SESSION.fetch_add(1, Ordering::Relaxed) + 1;
    info!("IPC: Client {} assigned session={}", peer_addr, conn_session);

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // --- Auth handshake (only when an auth token is configured) ---
    if let Some(expected) = AUTH_TOKEN.get() {
        match ws_rx.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<IpcAction>(&text) {
                    Ok(IpcAction::Auth { token }) if token == *expected => {
                        info!("IPC: Client {} authenticated session={}", peer_addr, conn_session);
                    }
                    _ => {
                        warn!("IPC: Client {} auth failed — closing session={}", peer_addr, conn_session);
                        let _ = ws_tx.close().await;
                        return;
                    }
                }
            }
            _ => {
                warn!("IPC: Client {} invalid auth handshake — closing", peer_addr);
                let _ = ws_tx.close().await;
                return;
            }
        }
    }

    // --- Send sandbox warnings to newly-connected client ---
    if let Some(warnings) = SANDBOX_WARNINGS.get() {
        if !warnings.is_empty() {
            let event = IpcEvent::SandboxWarning { commands: warnings.clone() };
            if let Ok(json) = wrap_event(&event) {
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    info!("IPC: Client {} disconnected during sandbox warning send", peer_addr);
                    return;
                }
            }
        }
    }

    // --- Per-client heartbeat and zombie detection ---
    let mut ping_ticker = interval(Duration::from_secs(CLIENT_PING_INTERVAL_S));
    ping_ticker.tick().await; // consume the immediate first tick
    let mut last_client_msg = Instant::now();

    loop {
        tokio::select! {
            // ── Heartbeat tick ────────────────────────────────────────────────
            _ = ping_ticker.tick() => {
                // Zombie detection: close clients that haven't sent anything.
                if last_client_msg.elapsed().as_secs() > CLIENT_IDLE_TIMEOUT_S {
                    warn!(
                        "IPC: Client {} session={} timed out ({}s idle) — closing",
                        peer_addr, conn_session, CLIENT_IDLE_TIMEOUT_S
                    );
                    let _ = ws_tx.close().await;
                    break;
                }
                // Send WebSocket Ping to detect dead TCP connections.
                if ws_tx.send(Message::Ping(vec![].into())).await.is_err() {
                    info!("IPC: Client {} disconnected during ping", peer_addr);
                    break;
                }
            }

            // ── Forward events to client ──────────────────────────────────────
            event_result = event_rx.recv() => {
                match event_result {
                    Ok(event) => {
                        let json = match wrap_event(&event) {
                            Ok(j) => j,
                            Err(e) => {
                                error!("IPC: Failed to serialize event: {}", e);
                                continue;
                            }
                        };
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            info!("IPC: Client {} disconnected (send failed)", peer_addr);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("IPC: Client {} lagged {} events — closing slow client", peer_addr, n);
                        // Close lagged clients to prevent unbounded queue growth.
                        let _ = ws_tx.close().await;
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("IPC: Broadcast channel closed");
                        break;
                    }
                }
            }

            // ── Receive messages from client ──────────────────────────────────
            msg_result = ws_rx.next() => {
                match msg_result {
                    Some(Ok(Message::Text(text))) => {
                        last_client_msg = Instant::now();
                        match serde_json::from_str::<IpcAction>(&text) {
                            Ok(action) => handle_action(action),
                            Err(e) => {
                                warn!("IPC: Invalid action from {}: {} ({})", peer_addr, text, e);
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        last_client_msg = Instant::now();
                        if ws_tx.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Client responded to our server-side ping — still alive.
                        last_client_msg = Instant::now();
                        debug!("IPC: Client {} pong received session={}", peer_addr, conn_session);
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("IPC: Client {} sent close frame session={}", peer_addr, conn_session);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("IPC: Error receiving from {}: {}", peer_addr, e);
                        break;
                    }
                    None => {
                        info!("IPC: Client {} stream ended", peer_addr);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("IPC: Client disconnected: {} session={}", peer_addr, conn_session);
}

pub fn has_clients() -> bool {
    if let Some(tx) = BROADCAST_TX.get() {
        tx.receiver_count() > 0
    } else {
        false
    }
}

/// Subscribe to the IPC broadcast channel.  Returns `None` if IPC has not
/// been initialized yet.  Used by the validation harness to capture IPC
/// events for assertion A010.
pub fn subscribe() -> Option<broadcast::Receiver<IpcEvent>> {
    BROADCAST_TX.get().map(|tx| tx.subscribe())
}