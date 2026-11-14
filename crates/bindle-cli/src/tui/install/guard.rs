//! Losing the terminal mid-install. Closing an SSH connection or a console
//! sends SIGHUP: Bindle keeps going without a screen (archinstall runs in a
//! session of its own and its output still reaches the log), so the disk is
//! never left half-installed by a dropped connection. SIGINT and SIGTERM
//! still end Bindle, but first remove the files that hold the passwords.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

static HUNG_UP: AtomicBool = AtomicBool::new(false);

/// The terminal went away (SIGHUP): nothing can be drawn or asked any more.
pub fn hung_up() -> bool {
    HUNG_UP.load(Ordering::Relaxed)
}

/// The files to remove if Bindle is told to stop.
#[derive(Clone)]
pub struct Guard(Arc<Mutex<Vec<PathBuf>>>);

impl Guard {
    /// Nothing left to remove (the caller removed them).
    pub fn clear(&self) {
        if let Ok(mut f) = self.0.lock() {
            f.clear();
        }
    }
}

fn remove(files: &Mutex<Vec<PathBuf>>) {
    if let Ok(mut f) = files.lock() {
        for p in f.drain(..) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// From now until Bindle ends: SIGHUP is noted, not fatal; SIGINT and
/// SIGTERM remove `files`, restore the terminal and end Bindle as the
/// signal would have. The handlers stay for the rest of the run.
pub fn arm(files: Vec<PathBuf>) -> Guard {
    let guard = Guard(Arc::new(Mutex::new(files)));
    let shared = guard.clone();
    let (ready, registered) = mpsc::channel();
    let _ = std::thread::Builder::new().name("install-signals".into()).spawn(move || {
        let Ok(mut signals) = Signals::new([SIGHUP, SIGINT, SIGTERM]) else { return };
        let _ = ready.send(());
        for signal in signals.forever() {
            if signal == SIGHUP {
                HUNG_UP.store(true, Ordering::Relaxed);
                continue;
            }
            remove(&shared.0);
            ratatui::restore();
            eprintln!("\nStopped. The passwords file was removed.");
            let _ = signal_hook::low_level::emulate_default_handler(signal);
            std::process::exit(128 + signal);
        }
    });
    // Registered before any file is written, so no signal slips past.
    let _ = registered.recv();
    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_forgets_the_files() {
        let dir = std::env::temp_dir().join(format!("bindle-guard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (kept, gone) = (dir.join("kept"), dir.join("gone"));
        std::fs::write(&kept, "x").unwrap();
        std::fs::write(&gone, "x").unwrap();
        let g = Guard(Arc::new(Mutex::new(vec![kept.clone()])));
        g.clear();
        remove(&g.0);
        assert!(kept.exists(), "cleared: not removed any more");
        let g = Guard(Arc::new(Mutex::new(vec![gone.clone()])));
        remove(&g.0);
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
