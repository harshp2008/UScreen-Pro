//! UScreen: High-performance Linux display extension and graphics tablet subsystem.
//!
//! UScreen turns Android tablets and smartphones into low-latency external monitors
//! and pressure-sensitive drawing tablets for Linux desktop environments (GNOME, KDE Plasma,
//! Hyprland, Cosmic, and X11).

mod capture;
mod cli;
mod commands;
mod config;
mod config_handler;
mod daemon;
mod display;
mod doctor;
#[cfg(feature = "inproc-encoder")]
mod encoder;
mod encoders;
mod guard;
mod input;
mod latency;
mod net;
mod options;
mod osk;
mod runtime;
mod session;
pub mod timing;
mod tray;
mod update;

// Used by `doctor` through `crate::is_network_serial`.
pub use net::adb::is_network_serial;

use anyhow::Result;
use clap::Parser;
use tracing::info;

use cli::{Cli, Commands};
use commands::{list_displays, show_status, stop_daemon};
use config_handler::client_control_cmd;
use daemon::run_daemon;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logging();

    match &cli.command {
        Some(Commands::Start { daemonize }) if *daemonize => {
            let exe = std::env::current_exe()?;
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("start");
            let home = std::env::var("HOME").unwrap_or_default();
            let log_dir = std::path::PathBuf::from(format!("{}/.local/share/uscreen", home));
            let _ = std::fs::create_dir_all(&log_dir);
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_dir.join("daemon.log"))?;
            let log_err = log.try_clone()?;
            cmd.stdout(log);
            cmd.stderr(log_err);
            cmd.stdin(std::process::Stdio::null());
            #[cfg(unix)]
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(|| {
                    libc::setsid();
                    libc::signal(libc::SIGHUP, libc::SIG_IGN);
                    Ok(())
                });
            }
            let child = cmd.spawn()?;
            println!("uscreen daemon started in background (PID: {})", child.id());
            return Ok(());
        }
        Some(Commands::Start { .. }) | None => {
            info!(
                "Starting uscreen daemon v{} (PID: {})",
                env!("GIT_VERSION_STRING"),
                std::process::id()
            );
            run_daemon(cli).await?;
        }
        Some(Commands::Stop) => stop_daemon().await?,
        Some(Commands::Status) => show_status().await?,
        Some(Commands::PauseDevice { serial }) => {
            client_control_cmd(&format!("PAUSE {}", serial)).await?
        }
        Some(Commands::ResumeDevice { serial }) => {
            client_control_cmd(&format!("RESUME {}", serial)).await?
        }
        Some(Commands::ListDisplays) => list_displays().await?,
        Some(Commands::Wifi { off }) => net::adb::setup_wifi(*off).await?,
        Some(Commands::Doctor {
            latency: true,
            secs,
            sparse,
        }) => doctor::run_latency(*secs, *sparse).await?,
        Some(Commands::CheckEncoder {
            encoder,
            vaapi_device,
        }) => commands::check_encoder(encoder, vaapi_device.as_deref()).await?,
        Some(Commands::Doctor { .. }) => doctor::run().await?,
        Some(Commands::ResetGuard) => match guard::reset() {
            Ok(true) => println!(
                "Safety record cleared. Restart the daemon to re-enable the virtual display."
            ),
            Ok(false) => println!("Nothing to clear: no safety record exists."),
            Err(e) => anyhow::bail!("could not clear {:?}: {}", guard::state_path(), e),
        },
    }

    Ok(())
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "uscreen=info".into()),
        )
        .with_target(true)
        .with_line_number(true)
        .init();
}
