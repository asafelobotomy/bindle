//! The desktop window asks for the password through pkexec; its failures must
//! reach the window as plain messages. A fake pkexec stands in (alone in this
//! test binary, because it changes PATH).

use std::fs;
use std::os::unix::fs::PermissionsExt;

use bindle_run::client::{HelperClient, Launch, NO_AGENT};

fn fake_pkexec(dir: &std::path::Path, message: &str, code: i32) {
    let script = format!("#!/bin/sh\necho '{message}' >&2\nexit {code}\n");
    let path = dir.join("pkexec");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn launch_error() -> String {
    let how = Launch { exe: Some("/bin/true".into()), graphical: true };
    match HelperClient::launch(None, "/dev/null", &how) {
        Ok(_) => panic!("the fake pkexec never starts a helper"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn pkexec_failures_are_reported_plainly() {
    let dir = std::env::temp_dir().join(format!("bindle-pkexec-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default());
    // SAFETY: this test binary runs this one test, on one thread.
    unsafe { std::env::set_var("PATH", path) };

    fake_pkexec(&dir, "Error executing command as another user: No authentication agent found.", 127);
    assert_eq!(launch_error(), NO_AGENT);

    fake_pkexec(&dir, "Error executing command as another user: Request dismissed", 126);
    assert!(launch_error().contains("Request dismissed"));

    let _ = fs::remove_dir_all(&dir);
}
