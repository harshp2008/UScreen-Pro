//! Tests of which app counts as an art app and of the per-app route overrides.

use super::support::*;
use crate::desktop::AppIdentity;
use super::super::route::{decide, route_to_pen, set_focus, toggle_route};
use std::collections::HashMap;

#[test]
fn art_apps_match_by_any_name_case_insensitively() {
    assert!(app("org.kde.krita.desktop", "krita").is_art(&art()));
    assert!(app("", "Gimp-2.10").is_art(&art()));
    assert!(!app("google-chrome.desktop", "Google-chrome").is_art(&art()));
    assert!(!AppIdentity::default().is_art(&art()));
}

#[test]
fn route_follows_the_app_then_the_override() {
    let krita = app("org.kde.krita.desktop", "krita");
    let chrome = app("google-chrome.desktop", "Google-chrome");
    let mut ov = HashMap::new();
    assert!(decide(Some(&krita), &ov, None, &art()));
    assert!(!decide(Some(&chrome), &ov, None, &art()));
    ov.insert(chrome.key().unwrap(), true);
    assert!(decide(Some(&chrome), &ov, None, &art()), "forced tablet for Chrome");
    ov.insert(krita.key().unwrap(), false);
    assert!(!decide(Some(&krita), &ov, None, &art()), "forced mouse for Krita");
    assert!(!decide(None, &ov, None, &art()), "unknown focus defaults to the mouse");
    assert!(decide(None, &ov, Some(true), &art()));
}

#[test]
fn toggling_remembers_the_choice_per_app() {
    set_focus(Some(app("zz-test-app.desktop", "zz-test-app")));
    assert!(!route_to_pen(&art()));
    assert!(toggle_route(&art()));
    assert!(route_to_pen(&art()));
    set_focus(Some(app("other", "other")));
    assert!(!route_to_pen(&art()), "another app is untouched");
    set_focus(Some(app("zz-test-app.desktop", "zz-test-app")));
    assert!(route_to_pen(&art()), "remembered");
    assert!(!toggle_route(&art()));
}
