use super::*;

#[test]
fn install_and_remove_into_a_root() {
    let root = crate::apply::sys::test_dir("firstlogin");
    let done = install(&root, true).unwrap();
    assert!(done.iter().any(|d| d.contains(BIN)));
    assert!(host(&root, AUTOSTART).exists() && host(&root, PROFILE_HOOK).exists());
    let entry = fs::read_to_string(host(&root, AUTOSTART)).unwrap();
    assert!(entry.contains("Exec=/usr/local/bin/bindle first-login run"));
    assert_eq!(fs::read_link(host(&root, USER_UNIT_WANTS)).unwrap(), PathBuf::from("../bindle-first-login.service"));
    let unit = fs::read_to_string(host(&root, USER_UNIT)).unwrap();
    assert!(unit.contains("ConditionUser=!@system"));
    assert!(fs::read_to_string(host(&root, PROFILE_HOOK)).unwrap().contains("UID_MIN"));
    let policy = fs::read_to_string(host(&root, POLKIT)).unwrap();
    // The helper's own path and first argument (client.rs: pkexec <bindle> helper), one approval each.
    assert!(policy.contains(&format!(">{BIN}<")) && policy.contains(">helper<"));
    assert!(policy.contains("<allow_active>auth_admin</allow_active>"));
    let tmpfiles = fs::read_to_string(host(&root, TMPFILES)).unwrap();
    assert!(tmpfiles.lines().any(|l| l == "r /etc/sudoers.d/zz-bindle-install"), "{tmpfiles}");
    // uQ Settings' page runs Bindle by a path under /usr/lib.
    assert!(fs::read_to_string(host(&root, UQ_PAGE)).unwrap().contains(&format!("\"{UQ_PAGE_BIN}\"")));
    assert!(uq_page::paths().iter().all(|p| host(&root, p).is_file()));
    assert_eq!(fs::read_link(host(&root, UQ_PAGE_BIN)).unwrap(), PathBuf::from(BIN));
    remove(&root, false).unwrap();
    assert!(!host(&root, AUTOSTART).exists() && host(&root, BIN).exists());
    assert!(host(&root, USER_UNIT_WANTS).symlink_metadata().is_err());
    // After a self-update: the page comes back as this version has it, the hooks stay off.
    fs::write(host(&root, UQ_PAGE), "stale").unwrap();
    fs::remove_file(host(&root, uq_page::DIAG)).unwrap();
    refresh(&root).unwrap();
    assert!(!fs::read_to_string(host(&root, UQ_PAGE)).unwrap().contains("stale"));
    assert!(host(&root, uq_page::DIAG).is_file());
    assert!(!host(&root, AUTOSTART).exists() && host(&root, USER_UNIT_WANTS).symlink_metadata().is_err());
    remove(&root, true).unwrap();
    assert!(!host(&root, BIN).exists() && !host(&root, POLKIT).exists());
    assert!(crate::bindo_cmd::NAMES.iter().all(|n| host(&root, n).symlink_metadata().is_err()));
    assert!(!host(&root, UQ_PAGE_DIR).exists() && host(&root, UQ_PAGE_BIN).symlink_metadata().is_err());
    assert!(!host(&root, uq_page::DIAG).exists());
    // Nothing installed: nothing written.
    assert!(refresh(&root).unwrap()[0].contains("isn't installed"));
    assert!(!host(&root, UQ_PAGE).exists());
}

/// The page only runs Bindle commands that exist, through the linked path, and every
/// declared command parses with the real argument parser (so a flag renamed in the CLI
/// breaks this test, not the page).
#[test]
fn the_uq_page_runs_real_subcommands() {
    let page: toml::Value = toml::from_str(uq_page::FILES[0].1).unwrap();
    assert_eq!(page["id"].as_str(), Some("bindle"));
    let commands = page["command"].as_array().unwrap();
    use clap::CommandFactory;
    let cli = crate::Cli::command();
    let example = |name: &str| match name {
        "stage" => "pacman",
        "id" => "20260101-1",
        "key" => "news_days",
        _ => "7",
    };
    for c in commands {
        let id = c["id"].as_str().unwrap();
        let mut args: Vec<String> =
            c["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect();
        assert_eq!(args[0], UQ_PAGE_BIN, "{id}");
        args[0] = "bindle".into();
        for a in c.get("args").and_then(|a| a.as_array()).into_iter().flatten() {
            args.push(example(a["name"].as_str().unwrap()).to_string());
        }
        cli.clone().try_get_matches_from(&args).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

/// Only UqLabel shows text (uQ refuses Text and Label), in every file of the page.
#[test]
fn the_uq_page_shows_text_only_through_uqlabel() {
    for (name, content) in uq_page::FILES.iter().filter(|(n, _)| n.ends_with(".qml")) {
        let plain = content.replace("UqLabel {", "");
        for refused in ["Text {", "Label {", "TextEdit", "TextInput", "createQmlObject", "import org.kde"] {
            assert!(!plain.contains(refused), "{name} uses {refused}");
        }
    }
}

#[test]
fn snippet_quotes_the_url_and_checks_the_digest() {
    let plain = archinstall_snippet("https://example.invalid/bindle", None).unwrap();
    assert!(plain.contains("first-login install") && !plain.contains("sha256sum"));
    let evil = archinstall_snippet("https://x/'; rm -rf / #", None).unwrap();
    let cmd: serde_json::Value = serde_json::from_str(&evil).unwrap();
    assert!(cmd["custom_commands"][0].as_str().unwrap().contains(r"'https://x/'\''; rm -rf / #'"));
    assert!(plain.contains("--proto '=https' --tlsv1.2"));
    let sum = "A".repeat(64);
    let checked = archinstall_snippet("https://x/b", Some(&sum)).unwrap();
    let cmd: serde_json::Value = serde_json::from_str(&checked).unwrap();
    let cmd = cmd["custom_commands"][0].as_str().unwrap();
    // Checked as a temporary file; only then executable and in place.
    let tmp = format!("{BIN}.download");
    let check = cmd.find(&format!("{}  {tmp}' | sha256sum -c", "a".repeat(64))).unwrap();
    assert!(cmd.find(&format!("-o {tmp} ")).unwrap() < check);
    assert!(check < cmd.find(&format!("chmod 755 {tmp}")).unwrap());
    assert!(check < cmd.find(&format!("mv -f {tmp} {BIN}")).unwrap());
    assert!(!cmd.contains(&format!("-o {BIN} ")) && cmd.ends_with(&format!("|| {{ rm -f {tmp}; false; }}")));
    assert!(archinstall_snippet("https://x/b", Some("abc")).is_err());
}

/// Once `/usr/local/bin/bindle` points at the AUR package (catalog module `bindle`), the uQ
/// page, bar icon, problem codes and `/usr/lib/bindle/bindle` are the package's: a refresh
/// leaves them alone, and a purge takes only the installer's own files.
#[test]
fn a_packaged_bindle_keeps_the_packages_files() {
    let root = crate::apply::sys::test_dir("firstlogin-packaged");
    install(&root, false).unwrap();
    assert!(!packaged(&root));
    let pkg = host(&root, PACKAGE_BIN);
    fs::create_dir_all(pkg.parent().unwrap()).unwrap();
    fs::write(&pkg, "package").unwrap();
    fs::remove_file(host(&root, BIN)).unwrap();
    symlink(&pkg, host(&root, BIN)).unwrap();
    assert!(packaged(&root));
    fs::write(host(&root, UQ_PAGE), "the package's").unwrap();
    assert!(refresh(&root).unwrap()[0].contains("package"));
    assert_eq!(fs::read_to_string(host(&root, UQ_PAGE)).unwrap(), "the package's");
    remove(&root, true).unwrap();
    assert!(host(&root, BIN).symlink_metadata().is_err() && !host(&root, POLKIT).exists());
    assert!(host(&root, UQ_PAGE).is_file() && host(&root, UQ_PAGE_BIN).symlink_metadata().is_ok());
    assert!(host(&root, uq_page::DIAG).is_file() && pkg.is_file());
}
