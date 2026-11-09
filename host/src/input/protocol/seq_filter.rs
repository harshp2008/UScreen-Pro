//! The per-connection sequence filter: drops duplicate or stale presses/lifts but never a move, and ignores the video frame acks

use super::*;

/// What a numbered input message is, as far as the sequence filter cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqKind {
    /// Pen/touch move or hover, relative motion, raw touchpad frames: idempotent, never dropped.
    Move,
    /// A press (pen/touch ACTION_DOWN).
    Down,
    /// A lift (pen/touch ACTION_UP, a cancel reaches the host as one).
    Up,
    /// Anything else that is numbered (buttons, gestures, hover exit...).
    Other,
}

/// Drops input messages that arrive twice, or a press/lift that arrives after the opposite one that
/// was sent later.
///
/// The tablet numbers every input message it sends with a `seq` that only goes up. The rules are
/// deliberately narrow, because dropping the wrong message loses a drag:
///
/// * A move or hover is **never** dropped (moving twice to the same place is harmless, and a move
///   that overtakes its DOWN must not make the drag vanish).
/// * Any other message is dropped if its exact `seq` was already accepted (a duplicate).
/// * A DOWN is dropped only if a later-numbered UP was already accepted (it is the stale half of a
///   stroke that already ended; pressing now would leave the button stuck down). Otherwise a DOWN
///   that arrives after later moves is still accepted, so the drag goes on.
/// * An UP is dropped only if a later-numbered DOWN was already accepted (it would cut the new
///   stroke short).
///
/// Messages without a `seq`, and messages that are not input (the `rendered` frame acks carry a
/// frame counter in their own `seq` field), always pass.
#[derive(Debug, Default)]
pub struct SeqFilter {
    last_down: Option<u64>,
    last_up: Option<u64>,
    recent: std::collections::VecDeque<u64>,
    /// How many messages were discarded on this connection.
    pub dropped: u64,
}

impl SeqFilter {
    /// Whether to let this message through (see the rules above); counts what it drops.
    pub fn accept(&mut self, msg: Option<(u64, SeqKind)>) -> bool {
        let Some((seq, kind)) = msg else { return true };
        if kind == SeqKind::Move {
            return true;
        }
        let stale = self.recent.contains(&seq)
            || match kind {
                SeqKind::Down => self.last_up.is_some_and(|u| seq < u),
                SeqKind::Up => self.last_down.is_some_and(|d| seq < d),
                _ => false,
            };
        if stale {
            self.dropped += 1;
            return false;
        }
        match kind {
            SeqKind::Down => self.last_down = self.last_down.max(Some(seq)),
            SeqKind::Up => self.last_up = self.last_up.max(Some(seq)),
            _ => {}
        }
        if self.recent.len() >= 64 {
            self.recent.pop_front();
        }
        self.recent.push_back(seq);
        true
    }
}

/// The input `seq` and kind of a raw message, if it is a numbered input message (cheap check
/// before parsing). `rendered` and every non-input type give `None`.
pub fn classify(text: &str) -> Option<(u64, SeqKind)> {
    if !text.contains("\"seq\"") {
        return None;
    }
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "type")]
        kind: Option<String>,
        seq: Option<u64>,
        action: Option<u8>,
    }
    let e = serde_json::from_str::<Envelope>(text).ok()?;
    let seq = e.seq?;
    let kind = match (e.kind.as_deref()?, e.action) {
        ("pen", Some(0)) | ("touch", Some(0)) => SeqKind::Down,
        ("pen", Some(1)) | ("touch", Some(1)) => SeqKind::Up,
        ("pen", Some(2 | 3)) | ("touch", Some(2)) => SeqKind::Move,
        ("rel" | "scroll" | "touchpad_raw", _) => SeqKind::Move,
        ("pen" | "touch" | "button" | "gesture", _) => SeqKind::Other,
        _ => return None,
    };
    Some((seq, kind))
}

#[cfg(test)]
mod seq_tests {
    use super::*;
    use SeqKind::*;

    #[test]
    fn increasing_numbers_pass() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((1, Down))) && f.accept(Some((2, Move))) && f.accept(Some((10, Up))));
        assert_eq!(f.dropped, 0);
    }

    #[test]
    fn a_duplicate_down_is_dropped() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((5, Down))));
        assert!(!f.accept(Some((5, Down))));
        assert_eq!(f.dropped, 1);
    }

    #[test]
    fn moves_are_never_dropped_even_repeated_or_late() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((9, Move))));
        assert!(f.accept(Some((9, Move))));
        assert!(f.accept(Some((3, Move))));
        assert_eq!(f.dropped, 0);
    }

    #[test]
    fn a_down_that_arrives_after_later_moves_still_starts_the_drag() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((6, Move)))); // sent after the DOWN, arrived first
        assert!(f.accept(Some((7, Move))));
        assert!(f.accept(Some((5, Down))), "the DOWN must not be lost");
        assert!(f.accept(Some((8, Up))));
    }

    #[test]
    fn a_stale_down_after_its_up_is_dropped() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((7, Up)))); // UP (sent second) arrives first
        assert!(!f.accept(Some((6, Down)))); // its DOWN arrives late: pressing now would stick
        assert!(f.accept(Some((8, Down))));
    }

    #[test]
    fn a_stale_up_after_a_newer_down_is_dropped() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((4, Down))));
        assert!(!f.accept(Some((3, Up))));
    }

    #[test]
    fn messages_without_a_number_always_pass() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((3, Down))));
        assert!(f.accept(None) && f.accept(None));
    }

    #[test]
    fn classify_reads_type_and_action() {
        assert_eq!(
            classify(r#"{"type":"pen","seq":42,"action":0}"#),
            Some((42, Down))
        );
        assert_eq!(
            classify(r#"{"type":"pen","seq":43,"action":2}"#),
            Some((43, Move))
        );
        assert_eq!(
            classify(r#"{"type":"pen","seq":44,"action":1}"#),
            Some((44, Up))
        );
        assert_eq!(
            classify(r#"{"type":"pen","seq":45,"action":5}"#),
            Some((45, Other))
        );
        assert_eq!(
            classify(r#"{"type":"rel","seq":46,"dx":1.0,"dy":0.0}"#),
            Some((46, Move))
        );
        assert_eq!(classify(r#"{"type":"pen","action":0}"#), None);
        assert_eq!(classify(r#"{"type":"pen","note":"seq","action":0}"#), None);
    }

    #[test]
    fn frame_acks_are_not_input_and_never_move_the_filter() {
        // `rendered` carries the video frame counter in `seq`; it used to be taken for an input
        // number, so once the counter was ahead every real pen message was discarded.
        assert_eq!(
            classify(r#"{"type":"rendered","seq":987654,"decode_us":300}"#),
            None
        );
        let mut f = SeqFilter::default();
        assert!(f.accept(classify(r#"{"type":"rendered","seq":987654}"#)));
        assert!(f.accept(classify(r#"{"type":"pen","seq":3,"action":0}"#)));
        assert_eq!(f.dropped, 0);
    }
}
