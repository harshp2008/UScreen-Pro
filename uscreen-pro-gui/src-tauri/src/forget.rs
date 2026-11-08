//! Forget a device: drop its profile, stop its session, disconnect it from adb and remove the
//! wireless pairing record the host holds for it.

use crate::adb::{self, norm_label};
use crate::config;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

/// What `forget_device` removed, reported back so the UI can show an honest summary.
#[derive(Serialize, Clone, Debug, Default)]
pub struct ForgetReport {
    /// Config keys removed (`tablets.<key>`).
    pub removed_profiles: Vec<String>,
    /// What the daemon did with the live session.
    pub session: String,
    /// One line per `adb disconnect` that was run, with adb's answer.
    pub disconnected: Vec<String>,
    /// Entries removed from `~/.android/adb_known_hosts.pb`.
    pub pairing_records_removed: usize,
    /// Anything that could not be done, or is deliberately left alone.
    pub notes: Vec<String>,
}

fn known_hosts_path() -> PathBuf {
    config::home_dir().join(".android/adb_known_hosts.pb")
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// `adb_known_hosts.pb` is `repeated { string name = 1; } entries = 1;` where `name` looks like
/// `adb-<hardware serial>-<suffix>`. Returns the file without the entries of `hw_serial`, plus how
/// many were dropped, or `None` when the file does not have exactly that shape (it is then left
/// untouched).
pub(crate) fn strip_known_host(data: &[u8], hw_serial: &str) -> Option<(Vec<u8>, usize)> {
    let prefix = format!("adb-{hw_serial}-");
    let mut out = Vec::with_capacity(data.len());
    let mut removed = 0;
    let mut pos = 0;
    while pos < data.len() {
        let entry_start = pos;
        if read_varint(data, &mut pos)? != 0x0a {
            return None;
        }
        let len = read_varint(data, &mut pos)? as usize;
        let body = data.get(pos..pos.checked_add(len)?)?;
        pos += len;
        let mut inner = 0;
        if read_varint(body, &mut inner)? != 0x0a {
            return None;
        }
        let name_len = read_varint(body, &mut inner)? as usize;
        let name = std::str::from_utf8(body.get(inner..inner.checked_add(name_len)?)?).ok()?;
        if name.starts_with(&prefix) {
            removed += 1;
        } else {
            out.extend_from_slice(&data[entry_start..pos]);
        }
    }
    Some((out, removed))
}

fn forget_pairing_record(hw_serial: &str, report: &mut ForgetReport) {
    let path = known_hosts_path();
    let Ok(data) = std::fs::read(&path) else {
        return;
    };
    match strip_known_host(&data, hw_serial) {
        Some((_, 0)) => {}
        Some((kept, n)) => {
            let tmp = path.with_extension("pb.tmp");
            let result = std::fs::write(&tmp, &kept).and_then(|_| std::fs::rename(&tmp, &path));
            match result {
                Ok(()) => report.pairing_records_removed = n,
                Err(e) => report
                    .notes
                    .push(format!("could not rewrite {}: {e}", path.display())),
            }
        }
        None => report.notes.push(format!(
            "{} has an unexpected layout; left untouched",
            path.display()
        )),
    }
}

async fn adb_disconnect(target: &str, report: &mut ForgetReport) {
    let line = match adb::run(None, &["disconnect", target], Duration::from_secs(8)).await {
        Ok(out) => format!("adb disconnect {target}: {}", adb::combined_of(&out)),
        Err(e) => format!("adb disconnect {target}: {e}"),
    };
    report.disconnected.push(line);
}

/// Forget `serial` (the hardware serial the profile is stored under).
#[tauri::command]
pub async fn forget_device(serial: String) -> Result<ForgetReport, String> {
    let mut report = ForgetReport::default();
    let mut cfg = config::load().await;

    // Every address this device has been reached through: the saved one, older profiles keyed by
    // `ip:port` that carry the same model label, and live Wi-Fi adb transports that answer with
    // this hardware serial.
    let mut addresses: BTreeSet<String> = BTreeSet::new();
    let profile = cfg.tablets.get(&serial).cloned();
    if let Some(addr) = profile
        .as_ref()
        .and_then(|p| p.wifi_address.clone())
        .filter(|a| !a.is_empty())
    {
        addresses.insert(addr);
    }
    let label = profile
        .as_ref()
        .map(|p| norm_label(&p.label))
        .unwrap_or_default();
    let mut remove_keys: Vec<String> = Vec::new();
    if profile.is_some() {
        remove_keys.push(serial.clone());
    }
    for (key, p) in &cfg.tablets {
        let legacy = key.contains(':') && *key != serial;
        if legacy && !label.is_empty() && norm_label(&p.label) == label {
            addresses.insert(key.clone());
            remove_keys.push(key.clone());
        }
    }
    for entry in adb::list_entries().await {
        if !entry.is_wifi {
            continue;
        }
        let known = addresses.contains(&entry.serial);
        let same_device = entry.state == "device"
            && adb::ro_serial(&entry.serial).await.as_deref() == Some(serial.as_str());
        if known || same_device {
            addresses.insert(entry.serial);
        }
    }

    // 1. config.toml: the profile, its legacy duplicates and any global address that points at it.
    for key in &remove_keys {
        cfg.tablets.remove(key);
    }
    if addresses.contains(&cfg.wifi_address) {
        cfg.wifi_address.clear();
        report
            .notes
            .push("cleared the global wifi_address that pointed at this device".into());
    }
    report.removed_profiles = remove_keys;
    config::store(&cfg).await?;

    // 2. Kill the live session (the config no longer lists the device, so it is not re-added).
    report.session = match crate::daemon::forget_session(&serial).await {
        Ok(msg) => msg,
        Err(e) => format!("daemon not reached: {e}"),
    };

    // 3. adb: drop every wireless transport so the reconnect watchdog and auto-connect cannot bring it back.
    for addr in &addresses {
        adb_disconnect(addr, &mut report).await;
    }
    let still_listed = adb::list_entries().await;
    for e in still_listed
        .iter()
        .filter(|e| e.is_wifi && e.serial.contains("._adb") && e.serial.contains(&serial))
    {
        adb_disconnect(&e.serial.clone(), &mut report).await;
    }

    // 4. Pairing record kept by the adb server for wireless-debugging (TLS) pairing.
    forget_pairing_record(&serial, &mut report);
    report.notes.push(
        "~/.android/adbkey is the host's own key shared by every device, so it is kept; the device itself lists this computer under Wireless debugging and can revoke it there"
            .into(),
    );
    let usb = adb::list_entries()
        .await
        .into_iter()
        .any(|e| !e.is_wifi && e.serial == serial && e.state == "device");
    if usb {
        report
            .notes
            .push("still plugged in over USB, so it is listed under New Devices".into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> Vec<u8> {
        let mut inner = vec![0x0a, name.len() as u8];
        inner.extend_from_slice(name.as_bytes());
        let mut e = vec![0x0a, inner.len() as u8];
        e.extend(inner);
        e
    }

    #[test]
    fn removes_only_the_forgotten_devices_record() {
        // The two records as they sit in a real adb_known_hosts.pb.
        let mut file = entry("adb-TABLET_SERIAL-kAXrKF");
        file.extend(entry("adb-PHONE_SERIAL-QwErTy"));
        let (kept, n) = strip_known_host(&file, "PHONE_SERIAL").unwrap();
        assert_eq!(n, 1);
        assert_eq!(kept, entry("adb-TABLET_SERIAL-kAXrKF"));
        let (same, none) = strip_known_host(&file, "NOSUCH").unwrap();
        assert_eq!((same, none), (file.clone(), 0));
    }

    #[test]
    fn a_serial_that_prefixes_another_is_not_matched() {
        let file = entry("adb-ABC1234-xyz");
        assert_eq!(strip_known_host(&file, "ABC12").unwrap().1, 0);
    }

    #[test]
    fn refuses_a_file_that_is_not_in_the_expected_shape() {
        assert!(strip_known_host(&[0x12, 0x02, 0x41, 0x42], "X").is_none());
        assert!(strip_known_host(&[0x0a, 0x7f, 0x01], "X").is_none());
    }
}
