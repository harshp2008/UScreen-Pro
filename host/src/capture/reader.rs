//! Reading the encoder's Annex B output: finds NAL boundaries, feeds the packetizer and
//! publishes finished access units (plus the codec config) to the stream server. CLI-encoder
//! path only; with the in-process encoder libavcodec hands back whole access units already.

use super::packetizer::H264AnnexBPacketizer;
use super::*;

impl CaptureManager {
    pub(super) async fn read_loop(
        mut stdout: impl tokio::io::AsyncRead + Unpin,
        tx: broadcast::Sender<VideoPacket>,
        codec_config: Arc<Mutex<Option<Bytes>>>,
        latency: crate::latency::LatencyTracker,
        codec: Codec,
        timing_shm: std::path::PathBuf,
        instance: u32,
    ) -> Result<()> {
        // A restart is reported as finished only once this generation has delivered a frame.
        let generation = crate::net::notices::encoder_started(instance);
        let mut buf = vec![0u8; 512 * 1024];
        let mut total: u64 = 0;
        let mut frames: u64 = 0;
        let mut last_log = Instant::now();
        let mut packetizer = H264AnnexBPacketizer::new(codec);
        let mut config_extracted = codec_config.lock().ok().and_then(|g| g.clone()).is_some();
        let timing_reader = crate::timing::TimingReader::open(&timing_shm);

        loop {
            let n = stdout
                .read(&mut buf)
                .await
                .context("Read error from encoder")?;

            if n == 0 {
                for data in packetizer.finish() {
                    if tx.receiver_count() > 0 {
                        let _ = tx.send(data);
                    }
                }
                return Ok(());
            }

            total += n as u64;

            let chunk = &buf[..n];
            let access_units = packetizer.push(chunk);

            if !config_extracted {
                if let Some(config) = packetizer.codec_config() {
                    info!("Extracted codec config (SPS+PPS): {} bytes", config.len());
                    if let Ok(mut cc) = codec_config.lock() {
                        *cc = Some(config);
                    }
                    config_extracted = true;
                } else if total > 1024 * 1024 {
                    warn!("Could not find SPS/PPS in first 1MB of stream");
                    config_extracted = true;
                }
            }

            let now_us = crate::timing::now_monotonic_us();
            for data in access_units {
                frames += 1;
                crate::net::notices::frame_emitted(instance, generation);
                if tx.receiver_count() > 0 {
                    let detailed = timing_reader
                        .as_ref()
                        .and_then(|tr| tr.get_record(data.seq))
                        .map(|rec| {
                            let req_to_ready_us =
                                rec.t_ready_us.saturating_sub(rec.t_request_us) as u32;
                            let grab_us = rec.t_grab_done_us.saturating_sub(rec.t_ready_us) as u32;
                            let conv_us =
                                rec.t_conv_done_us.saturating_sub(rec.t_grab_done_us) as u32;
                            let fifo_wait_write_us =
                                rec.t_fifo_write_us.saturating_sub(rec.t_conv_done_us) as u32;
                            let encode_us = now_us.saturating_sub(rec.t_fifo_write_us) as u32;
                            let host_total_us = now_us.saturating_sub(rec.t_ready_us) as u32;
                            crate::latency::DetailedTiming {
                                is_fresh: rec.is_fresh != 0,
                                req_to_ready_us,
                                grab_us,
                                conv_us,
                                fifo_wait_write_us,
                                encode_us,
                                host_total_us,
                            }
                        });
                    latency.on_encoded_with_timing(data.seq, detailed);
                    let _ = tx.send(data);
                }
            }
            latency.maybe_report();

            if last_log.elapsed().as_secs() >= 5 {
                let elapsed = last_log.elapsed().as_secs_f64();
                let mbps = if elapsed > 0.0 {
                    (total as f64 / elapsed) / 1_048_576.0
                } else {
                    0.0
                };
                let kbps = mbps * 8.0 * 1024.0;
                info!(
                    "Encoder: {} access units in {:.1}s, {:.1} MB/s ({:.0} kbps)",
                    frames, elapsed, mbps, kbps
                );
                frames = 0;
                total = 0;
                last_log = Instant::now();
            }
        }
    }
}
