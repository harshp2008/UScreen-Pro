//! Input path: a loopback WebSocket server that authenticates the tablet, answers its handshake
//! and turns its touch / pen / mouse / key messages into events on virtual uinput devices.
//!
//! uinput is used instead of compositor APIs so the same code works on X11, GNOME, KDE and
//! wlroots. Each tablet gets its own device set (see [`DeviceIdentity`]) and is mapped onto its
//! own virtual output, so two tablets never fight over one screen.

use crate::capture::EncoderSettings;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async_with_config, tungstenite::protocol::WebSocketConfig};
use tracing::{error, info, warn};

mod events;
mod mapping;
mod protocol;
mod server;
mod uinput;

pub use protocol::InputConfig;
pub use server::InputServer;

use events::*;
use mapping::*;
use protocol::*;
use uinput::*;
