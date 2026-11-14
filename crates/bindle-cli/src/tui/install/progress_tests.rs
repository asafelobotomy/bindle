use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn screen(p: &Progress, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| p.draw(f)).unwrap();
    term.backend().buffer().content().iter().map(|c| c.symbol()).collect()
}

#[test]
fn follows_the_install_through_its_steps() {
    let mut p = Progress::new("to /dev/vda".into(), "/var/log/x.log".into());
    assert_eq!(p.phase, 0);
    p.feed(b"Creating partitions: /dev/vda\nWaiting for time sync\n");
    assert_eq!(p.phase, 1);
    // Split across reads, with colour codes and a carriage-return redraw.
    p.feed(b"\x1b[1mInstalling pack");
    p.feed(b"ages: ['base']\x1b[0m\r\n downloading 10%\r downloading 90%\n");
    assert_eq!(p.phase, 2);
    assert_eq!(p.last_lines(2), [" downloading 10%", " downloading 90%"]);
    p.feed(b"Adding bootloader Systemd-boot to /dev/vda1\nCreating user solon\n");
    assert_eq!(p.phase, 4);
    // Earlier words later on don't move it back; counters only count in the last step.
    p.feed(b"Creating partitions: again\n[1/3] not yet\n");
    assert_eq!((p.phase, p.step.clone()), (4, None));
    p.feed(b"[bindle] Setting up the new system for solon (this can take a while: AUR builds)\xe2\x80\xa6\n");
    p.feed(b"[3/16] write /etc/sysctl.d/99-bindle-zram.conf [zram]\n");
    assert_eq!(p.phase, 5);
    assert_eq!(p.step.as_deref(), Some("Step 3 of 16: write /etc/sysctl.d/99-bindle-zram.conf [zram]"));
    let s = screen(&p, 120, 30);
    assert!(s.contains("Step 3 of 16") && s.contains("✓ Boot loader") && s.contains("to /dev/vda"), "{s}");
}

#[test]
fn last_screens() {
    let mut p = Progress::new("to /dev/vda".into(), "/var/log/x.log".into());
    p.feed(b"Installing packages: ['base']\nerror: failed retrieving file\n");
    p.end = Some(End::Failed("archinstall stopped (exit status: 1).".into()));
    for (w, h) in [(80, 25), (140, 45)] {
        let s = screen(&p, w, h);
        assert!(s.contains("✗ Arch base system") && s.contains("/var/log/x.log") && s.contains("failed retrieving"));
    }
    p.end = Some(End::Ready);
    let s = screen(&p, 80, 25);
    assert!(s.contains("Enter restart now") && s.contains("✓ Your apps and setup"));
    p.end = Some(End::Unfinished("Bindle opens when you first log in and finishes the rest.".into()));
    let s = screen(&p, 80, 25);
    assert!(s.contains("finishes the rest") && !s.contains("Enter restart"), "{s}");
}

#[test]
fn output_reaches_the_view_and_the_log() {
    let dir = std::env::temp_dir().join(format!("bindle-progress-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("install.log");
    let mut log = File::create(&path).ok();
    let mut child = Command::new("sh")
        .args(["-c", "echo 'Creating partitions: /dev/vda'; echo 'Installing packages: x'"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let rx = reader(child.stdout.take().unwrap());
    let mut p = Progress::new(String::new(), String::new());
    while drain(&rx, &mut p, &mut log) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.wait().unwrap().success());
    assert_eq!(p.phase, 2);
    drop(log);
    assert!(std::fs::read_to_string(&path).unwrap().contains("Installing packages: x"));
    // Another try appends, root-only.
    drop(open_log(&path));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("Installing packages: x") && text.contains("bindle install started"));
    let _ = std::fs::remove_dir_all(&dir);
}
