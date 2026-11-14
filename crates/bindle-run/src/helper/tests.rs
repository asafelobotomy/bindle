//! The helper's request handling in fake-commands mode: a temp root, and
//! commands only recorded as `would run:` lines.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::protocol::SnapshotPlan;

/// Events the helper wrote, shared with the test.
#[derive(Clone, Default)]
struct Sink(Rc<RefCell<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Sink {
    fn lines(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.0.borrow()).lines().map(String::from).collect()
    }
}

fn helper(name: &str) -> (Helper, Sink, PathBuf) {
    let root = crate::sys::test_dir(&format!("helper-{name}"));
    let sink = Sink::default();
    let opts = HelperOpts { root: root.clone(), fake_commands: true, log: "/var/log/bindle.log".into() };
    (Helper { opts, log: None, out: Box::new(sink.clone()), snapshot: None }, sink, root)
}

/// A user build directory beside (not inside) the helper's root.
fn build_dir(root: &Path) -> PathBuf {
    let dir = root.with_extension("build");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn check_names_refuses_options_and_odd_characters() {
    assert!(check_names(&["mako".into(), "lib32-mesa".into(), "getty@tty1.service".into()]).is_ok());
    for bad in ["-Rns", "", "a b", "a;b", "$(x)", "a/b"] {
        let err = check_names(&["ok".into(), bad.into()]).unwrap_err().to_string();
        assert!(err.contains("refusing invalid name"), "{bad}: {err}");
    }
}

#[test]
fn staged_packages_are_copied_then_installed_and_cleaned_up() {
    let (mut h, sink, root) = helper("stage");
    let build = build_dir(&root);
    let pkg = build.join("foo-1.0-1-x86_64.pkg.tar.zst");
    fs::write(&pkg, b"package bytes").unwrap();
    let copies = h.stage_packages(&[s(&pkg)]).unwrap();
    assert_eq!(copies, [format!("{STAGING}/foo-1.0-1-x86_64.pkg.tar.zst")]);
    let staged = host(&root, &copies[0]);
    assert_eq!(fs::read(&staged).unwrap(), b"package bytes");
    assert_eq!(fs::metadata(&staged).unwrap().permissions().mode() & 0o7777, 0o644);

    let ev = h.handle(Request::InstallFiles { paths: vec![s(&pkg)] }).unwrap();
    assert!(ev.is_none());
    let want = format!("would run: pacman -U --needed --noconfirm -- {STAGING}/foo-1.0-1-x86_64.pkg.tar.zst");
    assert!(sink.lines().iter().any(|l| l.contains(&want)), "{:?}", sink.lines());
    assert!(!host(&root, STAGING).exists(), "staged copies are removed after pacman");
}

#[test]
fn staging_refuses_a_symlink() {
    let (h, _, root) = helper("stage-symlink");
    let build = build_dir(&root);
    let target = build.join("real.pkg.tar.zst");
    fs::write(&target, b"x").unwrap();
    let link = build.join("foo-1-1-any.pkg.tar.zst");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let err = h.stage_packages(&[s(&link)]).unwrap_err().to_string();
    assert!(err.contains("not a package file"), "{err}");
    assert!(!host(&root, STAGING).join("foo-1-1-any.pkg.tar.zst").exists());
}

#[test]
fn staging_refuses_what_is_not_a_regular_file() {
    let (h, _, root) = helper("stage-irregular");
    let build = build_dir(&root);
    let dir = build.join("dir.pkg.tar.zst");
    fs::create_dir(&dir).unwrap();
    let fifo = build.join("fifo.pkg.tar.zst");
    let c = std::ffi::CString::new(s(&fifo)).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    for p in [&dir, &fifo, &build.join("missing.pkg.tar.zst")] {
        let err = h.stage_packages(&[s(p)]).unwrap_err().to_string();
        assert!(err.contains("not a package file"), "{}: {err}", p.display());
    }
}

#[test]
fn staging_refuses_bad_names_and_paths() {
    let (h, _, root) = helper("stage-names");
    let build = build_dir(&root);
    for name in ["notes.txt", ".hidden.pkg.tar.zst"] {
        fs::write(build.join(name), b"x").unwrap();
        let err = h.stage_packages(&[s(&build.join(name))]).unwrap_err().to_string();
        assert!(err.contains("not a package file"), "{name}: {err}");
    }
    let relative = h.stage_packages(&["foo.pkg.tar.zst".into()]).unwrap_err().to_string();
    assert!(relative.contains("unsafe path"), "{relative}");
    let dotdot = format!("{}/../foo.pkg.tar.zst", build.display());
    assert!(h.stage_packages(&[dotdot]).unwrap_err().to_string().contains("unsafe path"));
}

#[test]
fn install_files_fails_without_running_pacman_on_a_bad_file() {
    let (mut h, sink, root) = helper("install-bad");
    let build = build_dir(&root);
    let link = build.join("foo-1-1-any.pkg.tar.zst");
    std::os::unix::fs::symlink("/etc/hostname", &link).unwrap();
    assert!(h.handle(Request::InstallFiles { paths: vec![s(&link)] }).is_err());
    assert!(!sink.lines().iter().any(|l| l.contains("would run: pacman")), "{:?}", sink.lines());
}

#[test]
fn restore_reports_each_item_it_could_not_put_back_and_stays_not_undone() {
    let (mut h, sink, root) = helper("restore");
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc/a.conf"), "a\n").unwrap();
    fs::write(root.join("etc/b.conf"), "b\n").unwrap();
    let plan = SnapshotPlan {
        files: vec!["/etc/a.conf".into(), "/etc/b.conf".into()],
        units: vec!["-bad.service".into()],
        ..Default::default()
    };
    let Some(Event::Snapshot { id }) = h.handle(Request::Snapshot { plan: Box::new(plan) }).unwrap() else {
        panic!("no snapshot")
    };
    fs::write(root.join("etc/b.conf"), "changed\n").unwrap();
    // a.conf became a symlink: it can't be put back, b.conf still is.
    fs::remove_file(root.join("etc/a.conf")).unwrap();
    std::os::unix::fs::symlink("/dev/null", root.join("etc/a.conf")).unwrap();

    let err = h.handle(Request::Restore { id: id.clone() }).unwrap_err().to_string();
    assert!(err.contains("2 item(s) could not be restored"), "{err}");
    assert_eq!(fs::read_to_string(root.join("etc/b.conf")).unwrap(), "b\n");
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("not restored: /etc/a.conf")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("not restored: disable units: refusing invalid name")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("would run: systemctl disable")), "{lines:?}");
    assert!(!snapshot::load(&root, &id).unwrap().undone);
}

#[test]
fn restore_of_an_unknown_snapshot_fails() {
    let (mut h, _, _) = helper("restore-unknown");
    assert!(h.handle(Request::Restore { id: "../etc".into() }).is_err());
}
