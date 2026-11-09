//! `uscreen doctor --latency`: end-to-end latency report and the mouse sweep

use super::*;

/// `uscreen doctor --latency`: move a synthetic mouse across the whole desktop
/// for `secs` seconds (so the virtual screen keeps producing frames) and ask the
/// running daemon for the percentiles of what the tablet acknowledged meanwhile.
///
/// What is measured is the frame path the cursor rides on: encoder output ->
/// tablet screen (`encode_to_display`, which includes the wire and the tablet's
/// decode) and capture-ready -> screen (`true_e2e`). The desktop mouse itself
/// never reaches the daemon, so it has no timestamp of its own to log.
pub async fn run_latency(secs: u64, sparse: bool) -> anyhow::Result<()> {
    let secs = secs.clamp(3, 60);
    let cfg = FileConfig::load();
    println!(
        "Latency test: {} s of {} mouse motion across both screens; configured {}x{}@{} {}, stream scale {}",
        secs,
        if sparse { "sparse (one step per 100 ms)" } else { "continuous" },
        cfg.width, cfg.height, cfg.fps, cfg.encoder, cfg.stream_scale
    );
    let sweeper = tokio::task::spawn_blocking(move || sweep_mouse(secs, sparse));
    let sock = crate::daemon::runtime::runtime_dir().join("control.sock");
    // Give the sweep a moment to start, then read the window that covers it.
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    let reply = {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let mut s = tokio::net::UnixStream::connect(&sock).await.map_err(|e| {
            anyhow::anyhow!(
                "daemon control socket {:?}: {} (is the daemon running?)",
                sock,
                e
            )
        })?;
        s.write_all(format!("LATENCY {}\n", secs).as_bytes())
            .await?;
        let mut line = String::new();
        tokio::io::BufReader::new(s).read_line(&mut line).await?;
        line
    };
    match sweeper.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => println!("(mouse sweep: {e})"),
        Err(e) => println!("(mouse sweep task failed: {e})"),
    }
    let v: serde_json::Value = serde_json::from_str(reply.trim())?;
    let n = v["samples"].as_u64().unwrap_or(0);
    if n == 0 {
        println!("No frames were acknowledged by a tablet in that window. Attach a tablet with the UScreen app open and retry.");
        return Ok(());
    }
    println!("frames acknowledged by the tablet: {n}");
    println!(
        "encoder -> tablet screen : p50 {:.1} ms   p95 {:.1} ms",
        v["encode_to_display_p50_ms"].as_f64().unwrap_or(0.0),
        v["encode_to_display_p95_ms"].as_f64().unwrap_or(0.0)
    );
    println!(
        "capture-ready -> screen  : p50 {:.1} ms   p95 {:.1} ms   ({} frames with stage timing)",
        v["true_e2e_p50_ms"].as_f64().unwrap_or(0.0),
        v["true_e2e_p95_ms"].as_f64().unwrap_or(0.0),
        v["true_e2e_samples"].as_u64().unwrap_or(0)
    );
    println!(
        "host stages only         : p50 {:.1} ms   p95 {:.1} ms",
        v["host_stages_p50_ms"].as_f64().unwrap_or(0.0),
        v["host_stages_p95_ms"].as_f64().unwrap_or(0.0)
    );
    Ok(())
}

/// A relative uinput mouse that sweeps left-right over more than the whole
/// desktop (the pointer clamps at the edges) at ~125 Hz.
pub(super) fn sweep_mouse(secs: u64, sparse: bool) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
    const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
    const UI_SET_RELBIT: libc::c_ulong = 0x4004_5566;
    const UI_DEV_CREATE: libc::c_ulong = 0x5501;
    const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .map_err(|e| anyhow::anyhow!("cannot open /dev/uinput: {e}"))?;
    let fd = f.as_raw_fd();
    unsafe {
        for ev in [0u32, 1, 2] {
            libc::ioctl(fd, UI_SET_EVBIT, ev);
        }
        libc::ioctl(fd, UI_SET_KEYBIT, 0x110u32);
        libc::ioctl(fd, UI_SET_RELBIT, 0u32);
        libc::ioctl(fd, UI_SET_RELBIT, 1u32);
    }
    // struct uinput_user_dev: name[80], id{bus,vendor,product,version}, ff_effects_max, 4 x i32[64]
    let mut dev = vec![0u8; 80 + 8 + 4 + 4 * 256];
    let name = b"uscreen-latency-probe";
    dev[..name.len()].copy_from_slice(name);
    dev[80..82].copy_from_slice(&3u16.to_ne_bytes());
    f.write_all(&dev)?;
    unsafe {
        libc::ioctl(fd, UI_DEV_CREATE);
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    let ev = |f: &mut std::fs::File, t: u16, c: u16, v: i32| -> std::io::Result<()> {
        let mut b = [0u8; 24];
        b[16..18].copy_from_slice(&t.to_ne_bytes());
        b[18..20].copy_from_slice(&c.to_ne_bytes());
        b[20..24].copy_from_slice(&v.to_ne_bytes());
        f.write_all(&b)
    };
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs.saturating_sub(1));
    let (mut dir, mut n) = (1i32, 0u32);
    while std::time::Instant::now() < end {
        ev(&mut f, 2, 0, dir * 45)?;
        ev(&mut f, 2, 1, if n % 2 == 0 { 3 } else { -3 })?;
        ev(&mut f, 0, 0, 0)?;
        n += 1;
        if n % if sparse { 22 } else { 220 } == 0 {
            dir = -dir;
        }
        std::thread::sleep(std::time::Duration::from_millis(if sparse {
            100
        } else {
            8
        }));
    }
    unsafe {
        libc::ioctl(fd, UI_DEV_DESTROY);
    }
    Ok(())
}
