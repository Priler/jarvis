use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::events::{IpcAction, IpcEvent};

pub const IPC_PORT: u16 = 9712;
pub const IPC_ADDR: &str = "127.0.0.1";

static BROADCAST_TX: OnceCell<broadcast::Sender<IpcEvent>> = OnceCell::new();
static ACTION_HANDLER: OnceCell<Arc<RwLock<Option<Box<dyn Fn(IpcAction) + Send + Sync>>>>> =
    OnceCell::new();

// Initialize the IPC broadcast channel
pub fn init() -> broadcast::Sender<IpcEvent> {
    if let Some(tx) = BROADCAST_TX.get() {
        return tx.clone();
    }

    let (tx, _) = broadcast::channel::<IpcEvent>(32);
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

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    loop {
        tokio::select! {
            // forward events to client
            event_result = event_rx.recv() => {
                match event_result {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
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
                        warn!("IPC: Client {} lagged {} events", peer_addr, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("IPC: Broadcast channel closed");
                        break;
                    }
                }
            }

            // receive messages from client
            msg_result = ws_rx.next() => {
                match msg_result {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<IpcAction>(&text) {
                            Ok(action) => handle_action(action),
                            Err(e) => {
                                warn!("IPC: Invalid action from {}: {} ({})", peer_addr, text, e);
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if ws_tx.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("IPC: Client {} sent close frame", peer_addr);
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

    info!("IPC: Client disconnected: {}", peer_addr);
}

pub fn has_clients() -> bool {
    if let Some(tx) = BROADCAST_TX.get() {
        tx.receiver_count() > 0
    } else {
        false
    }
}
