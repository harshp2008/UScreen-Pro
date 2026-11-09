//! Multi-device session state machine, display pipeline orchestration, and lifecycle tracking.
//!
//! This module coordinates independent streaming pipelines for primary and secondary tablet
//! connections. Each active device is allocated an EVDI display device node, video encoder,
//! network streaming server, and uinput input receiver, isolated from other concurrent sessions.
//!
//! Layout:
//! * `types` - the control messages and the records describing extra (non-primary) tablet sessions
//! * `extra` - starting the pipeline of a second (third, ...) tablet
//! * `profile` - applying a tablet's saved profile (stream settings) when it connects
//! * `pipeline` - helpers to find and signal a tablet's capture pipeline (FIFO, slot, signals)
//! * `monitor` - the adb monitor: the long-running loop that watches for tablets and drives every session's lifecycle

mod extra;
mod monitor;
mod pipeline;
mod profile;
mod types;
use extra::*;
pub use monitor::*;
use pipeline::*;
use profile::*;
pub use types::*;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Notify};
use tracing::{error, info, warn};

use crate::capture::{self, CaptureConfig, CaptureManager, EncoderSettings};
use crate::config::{FileConfig, Position};
use crate::display::vdisplay;
use crate::input::{InputConfig, InputServer};
use crate::net::adb::{
    announce_transport, app_running, auto_provision_wifi, is_fake_serial, launch_app,
    on_tablet_connected, scan_adb_devices, setup_adb_forwarding, spawn_adb_tracker,
    teardown_adb_forwarding, DiscoveredDevice, Transport,
};
use crate::net::stream::{StreamConfig, StreamServer};
