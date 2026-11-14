//! JSON-lines protocol between the user-side orchestrator and the root helper.

use bindle_core::Step;
use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    /// Record everything this apply is about to touch.
    Snapshot {
        plan: Box<SnapshotPlan>,
    },
    /// Execute one privileged plan step.
    Run {
        step: Step,
    },
    /// `pacman -U` package files built as the user (AUR bootstrap).
    InstallFiles {
        paths: Vec<String>,
    },
    /// Run a command as root on the user's terminal (by path), so it can
    /// ask its own questions; output goes there, not to the protocol.
    Interactive {
        argv: Vec<String>,
        tty: String,
    },
    /// The names of the files in /boot, one `Output` each (often root-only).
    BootFiles,
    /// Replace Bindle's own programs with a verified release (bindleupgrade).
    InstallBindle {
        files: Vec<BindleFile>,
    },
    /// Put back the files recorded in a snapshot and disable its units.
    /// Files changed since the apply wrote them are kept, unless `force`.
    Restore {
        id: String,
        #[serde(default)]
        force: bool,
    },
    /// A line for the log from the user side: steps that run as the user
    /// (AUR builds, release downloads, user commands) and how they ended.
    Log {
        line: String,
    },
    /// The apply that took this snapshot finished every step.
    Complete {
        id: String,
    },
    Quit,
}

/// A downloaded release file (owned by the user), where it goes, and the
/// sha256 the signed SHA256SUMS lists for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindleFile {
    pub src: String,
    pub dest: String,
    pub sha256: String,
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
    /// Packages installed from Bindle's pinned releases (also in `packages`):
    /// bindleupgrade updates these from their releases.
    #[serde(default)]
    pub releases: Vec<String>,
    /// The Bindle making this apply, what each of its modules puts on the
    /// system, and the modules it retires (for "What's new").
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub record: std::collections::BTreeMap<String, bindle_core::plan::content::Content>,
    #[serde(default)]
    pub forget: Vec<String>,
    /// Directories the apply creates (mount points); undo removes the ones
    /// that didn't exist, once they are empty.
    #[serde(default)]
    pub dirs: Vec<String>,
}
