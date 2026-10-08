//! JSON-lines protocol between the user-side orchestrator and the root helper.

use bindle_core::Step;
use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    /// Record everything this apply is about to touch.
    Snapshot {
        plan: SnapshotPlan,
    },
    /// Execute one privileged plan step.
    Run {
        step: Step,
    },
    /// `pacman -U` package files built as the user (AUR bootstrap).
    InstallFiles {
        paths: Vec<String>,
    },
    /// Put back the files recorded in a snapshot and disable its units.
    Restore {
        id: String,
    },
    Quit,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    Hello { version: String },
    Output { line: String },
    Snapshot { id: String },
    Done,
    Failed { error: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotPlan {
    pub label: String,
    pub user: String,
    pub files: Vec<String>,
    pub units: Vec<String>,
    pub packages: Vec<String>,
    pub aur: Vec<String>,
    #[serde(default)]
    pub manual: Vec<String>,
}
