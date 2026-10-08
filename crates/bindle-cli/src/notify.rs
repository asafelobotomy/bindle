//! "Bindle hasn't been set up yet" reminder, as a freedesktop notification
//! with actions. Talks D-Bus directly so it works without libnotify.

use std::collections::HashMap;

use anyhow::{Context, Result};
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::Value;
use zbus::{MatchRule, Message};

const DEST: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

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

fn signal_for(msg: &Message, id: u32) -> Option<Choice> {
    let header = msg.header();
    let member = header.member()?.to_string();
    match member.as_str() {
        "ActionInvoked" => {
            let (got, action): (u32, String) = msg.body().deserialize().ok()?;
            (got == id).then(|| choice_for(&action))
        }
        "NotificationClosed" => {
            let (got, _reason): (u32, u32) = msg.body().deserialize().ok()?;
            (got == id).then_some(Choice::Ignored)
        }
        _ => None,
    }
}

/// Show the reminder and block until the user acts on it or it closes.
/// Errors when there is no session bus or notification server.
pub fn remind() -> Result<Choice> {
    let conn = Connection::session().context("no D-Bus session bus")?;
    // Subscribe before sending so a fast click can't be missed.
    let rule = MatchRule::builder().msg_type(Type::Signal).interface(DEST)?.path(PATH)?.build();
    let mut signals = MessageIterator::for_match_rule(rule, &conn, Some(16))?;
    let proxy = Proxy::new(&conn, DEST, PATH, DEST)?;

    let actions = ["default", "Set up now", "open", "Set up now", "dismiss", "Don't remind me"];
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("desktop-entry", Value::from("bindle"));
    hints.insert("resident", Value::from(true));
    hints.insert("urgency", Value::from(1u8));
    let id: u32 = proxy
        .call(
            "Notify",
            &(
                "Bindle",
                0u32,
                "bindle",
                "Finish setting up your system",
                "Bindle hasn't been set up yet. Pick drivers, apps and tweaks for this machine.",
                actions.as_slice(),
                hints,
                0i32, // stay until handled (kept in the notification tray)
            ),
        )
        .context("no notification server")?;

    for msg in signals.by_ref() {
        if let Some(choice) = msg.ok().as_ref().and_then(|m| signal_for(m, id)) {
            return Ok(choice);
        }
    }
    Ok(Choice::Ignored)
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
