pub mod mock;
pub mod parser;
pub mod session;
pub mod telnet;
pub mod websocket;
pub mod ws_session;

use std::sync::atomic::AtomicU64;

/// Shared monotonic counter so telnet and WebSocket clients never get the same ID.
pub(super) static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);
