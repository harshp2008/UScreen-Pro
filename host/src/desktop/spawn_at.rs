//! Thin wrapper around the `spawn-at` command (https://github.com/harshp2008/spawn-at): put a new
//! window at an exact spot (the pointer) on GNOME Wayland, list windows, and hand focus back.
//! Used by the GNOME backend; nothing else in the host calls `spawn-at`.

use std::process::{Command, Stdio};

/// A window as `spawn-at` names it (its numeric id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowRef(pub String);

/// Whether `spawn-at` is on `$PATH`.
pub(super) fn installed() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| std::path::Path::new(d).join("spawn-at").exists())
}

/// The focused window in the output of `spawn-at query windows`. The FOCUSED column is found from
/// the header line, because titles contain spaces and cannot be split off by whitespace.
pub(super) fn parse_focused(table: &str) -> Option<WindowRef> {
    let mut lines = table.lines();
    let header = lines.next()?;
    let col = header.find("FOCUSED")?;
    lines
        .find(|l| l.get(col..col + 1) == Some("*"))
        .and_then(|l| l.split_whitespace().next())
        .map(|id| WindowRef(id.to_string()))
}

/// The window that has focus now.
pub(super) fn focused_window() -> Option<WindowRef> {
    let out = Command::new("spawn-at")
        .args(["query", "windows"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| parse_focused(&String::from_utf8_lossy(&out.stdout)))
        .flatten()
}

/// Give focus to `w`. True if `spawn-at` succeeded.
pub(super) fn focus_window(w: &WindowRef) -> bool {
    Command::new("spawn-at")
        .args(["focus", "--id", &w.0])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Start `command` with its window centred on the pointer, `lift` pixels above it (the work area
/// clamps it to the screen). `class` is the window's app id: without it `spawn-at` does not find a
/// GTK 4 window and leaves it where the compositor put it (measured). Returns once the window is
/// placed, about 0.2 s; the program keeps running.
pub(super) fn spawn_at_pointer(lift: i32, class: &str, command: &[String]) -> bool {
    let mut cmd = Command::new("spawn-at");
    cmd.args([
        "spawn", "--class", class, "--anchor", "cursor", "--pivot", "center", "--offset", "0",
    ])
    .arg((-lift).to_string())
    .arg("--")
    .args(command)
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    cmd.status().is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
ID         PID      CLASS                 MON  STATE      GEOMETRY        REL  FOCUSED  TITLE
296732775  1425580  google-chrome         0    maximized  1920x1040@0,40  0,0           A page - Google Chrome
296732771  1422374  com.anthropic.Claude  0    maximized  1550x1040@0,40  0,0  *        Claude Code
";

    #[test]
    fn finds_the_focused_window_by_column() {
        assert_eq!(parse_focused(TABLE), Some(WindowRef("296732771".into())));
    }

    #[test]
    fn no_focused_window_is_none() {
        let t = TABLE.replace('*', " ");
        assert_eq!(parse_focused(&t), None);
        assert_eq!(parse_focused(""), None);
    }

    #[test]
    fn a_star_in_a_title_is_not_a_focus_marker() {
        let t = TABLE.replace("A page - Google Chrome", "* not focused");
        assert_eq!(parse_focused(&t), Some(WindowRef("296732771".into())));
    }
}
