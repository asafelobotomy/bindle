//! "Bindle hasn't been set up yet" reminder, as a freedesktop notification
//! with actions. Talks D-Bus directly so it works without libnotify.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::Value;
use zbus::{MatchRule, Message};

const DEST: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
/// Give up waiting after this long, so the login process doesn't linger.
const WAIT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    /// "Set up now" (or the notification body was clicked).
    Open,
    /// "Don't remind me".
    Dismiss,
    /// Closed or expired without a choice.
    Ignored,
}

/// Map a notification action key to the user's choice.
pub fn choice_for(action: &str) -> Choice {
    match action {
        "default" | "open" => Choice::Open,
        "dismiss" => Choice::Dismiss,
        _ => Choice::Ignored,
    }
}

/// For notification `id`: Some(Some(key)) a button, Some(None) closed.
fn signal_for(msg: &Message, id: u32) -> Option<Option<String>> {
    let header = msg.header();
    let member = header.member()?.to_string();
    match member.as_str() {
        "ActionInvoked" => {
            let (got, action): (u32, String) = msg.body().deserialize().ok()?;
            (got == id).then_some(Some(action))
        }
        "NotificationClosed" => {
            let (got, _reason): (u32, u32) = msg.body().deserialize().ok()?;
            (got == id).then_some(None)
        }
        _ => None,
    }
}

/// Show the reminder and block until the user acts on it, it closes, or
/// `WAIT` passes. Errors when there is no session bus, no notification server,
/// or one that can't show buttons (the caller then opens the wizard instead).
pub fn remind() -> Result<Choice> {
    let actions = [("open", "Set up now"), ("dismiss", "Don't remind me")];
    let action = show(
        "Finish setting up your system",
        "Bindle hasn't been set up yet. Pick drivers, apps and tweaks for this machine.",
        &actions,
    )?;
    Ok(action.as_deref().map_or(Choice::Ignored, choice_for))
}

/// A notification with buttons (`(key, label)`; clicking the body counts as
/// the first), through whatever notification server runs (on uQ, noti-fi).
/// Blocks until a button is pressed (its key), it closes or `WAIT` passes (None).
pub fn show(summary: &str, body: &str, buttons: &[(&str, &str)]) -> Result<Option<String>> {
    let conn = Connection::session().context("no D-Bus session bus")?;
    let proxy = Proxy::new(&conn, DEST, PATH, DEST)?;
    let caps: Vec<String> = proxy.call("GetCapabilities", &()).context("no notification server")?;
    if !caps.iter().any(|c| c == "actions") {
        bail!("the notification server doesn't support buttons");
    }
    // Subscribe before sending so a fast click can't be missed.
    let rule = MatchRule::builder().msg_type(Type::Signal).interface(DEST)?.path(PATH)?.build();
    let signals = MessageIterator::for_match_rule(rule, &conn, Some(16))?;

    let mut actions: Vec<&str> = vec![];
    if let Some((_, label)) = buttons.first() {
        actions.extend(["default", label]);
    }
    for (key, label) in buttons {
        actions.extend([*key, *label]);
    }
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("desktop-entry", Value::from("bindle"));
    hints.insert("resident", Value::from(true));
    hints.insert("urgency", Value::from(1u8));
    let id: u32 = proxy
        .call("Notify", &("Bindle", 0u32, "bindle", summary, body, actions.as_slice(), hints, 0i32))
        .context("no notification server")?;

    // The iterator blocks with no timeout of its own: wait on it in a thread.
    let first = buttons.first().map(|(k, _)| k.to_string());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let action = signals.into_iter().find_map(|msg| msg.ok().as_ref().and_then(|m| signal_for(m, id)));
        let _ = tx.send(action.flatten());
    });
    match rx.recv_timeout(WAIT) {
        Ok(Some(a)) if a == "default" => Ok(first),
        Ok(a) => Ok(a),
        Err(_) => {
            // Don't leave a button behind that nothing is listening to.
            let _: Result<(), _> = proxy.call("CloseNotification", &(id,));
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_actions() {
        assert_eq!(choice_for("default"), Choice::Open);
        assert_eq!(choice_for("open"), Choice::Open);
        assert_eq!(choice_for("dismiss"), Choice::Dismiss);
        assert_eq!(choice_for("something-else"), Choice::Ignored);
    }
}
