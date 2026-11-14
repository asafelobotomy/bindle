//! What the You page must hold before anything is erased: names, matching
//! passwords and passphrases, and a timezone, keyboard and language the new
//! system has. Also the plan's AUR packages, for Review.

use bindle_core::install::target::{self, Answers};
use bindle_core::plan::{self, Step as PlanStep};

use super::{Key, Wizard};
use crate::livesys::{self, Disk};

impl Wizard<'_> {
    /// Why the You page isn't complete, or the answers.
    pub fn answers(&self) -> Result<Answers, String> {
        let disk = match (&self.manual, self.chosen_disk()) {
            (Some(m), _) => Disk { path: m.devices(), ..Disk::default() },
            (None, Some(d)) => d.clone(),
            (None, None) => return Err("choose a disk".into()),
        };
        let v = |k| self.value(k).trim().to_string();
        let keyboard = v(Key::Keyboard);
        if self.keymaps.as_ref().is_some_and(|maps| !target::valid_keymap(&keyboard, maps)) || keyboard.is_empty() {
            return Err(format!("Keyboard layout `{keyboard}` isn't known; e.g. us, uk, de-latin1, fr"));
        }
        if !target::valid_hostname(&v(Key::Hostname)) {
            return Err("Computer name: lower-case letters, digits and -, up to 63".into());
        }
        let user = v(Key::Username);
        if !target::valid_username(&user) {
            return Err(
                "User name: lower-case letters, digits, - and _, starting with a letter (not a system name)".into()
            );
        }
        if target::username_in_passwd(&user, &self.passwd) {
            return Err(format!("User name `{user}` belongs to the system; choose another"));
        }
        if self.value(Key::Password).is_empty() || self.value(Key::Password) != self.value(Key::Confirm) {
            return Err("The two passwords must match and can't be empty".into());
        }
        if self.encrypting() && self.value(Key::DiskPassphrase).chars().count() < 8 {
            return Err("Disk passphrase: at least 8 characters".into());
        }
        if self.encrypting() && self.value(Key::DiskPassphrase) != self.value(Key::DiskConfirm) {
            return Err("The two disk passphrases must match".into());
        }
        if !livesys::valid_timezone(&v(Key::Timezone)) {
            return Err(format!("Timezone `{}` isn't known; use Area/City, e.g. Europe/London", v(Key::Timezone)));
        }
        let lang = v(Key::Language);
        if lang.is_empty() || self.locales.as_deref().is_some_and(|s| !target::valid_locale(&lang, s)) {
            return Err(format!("Language `{lang}` isn't known; e.g. en_US, en_GB, de_DE, fr_FR"));
        }
        Ok(Answers {
            disk: disk.path,
            disk_bytes: disk.bytes,
            filesystem: self.filesystem,
            encryption: match &self.manual {
                Some(m) => m.passphrase.clone(),
                None => self.encrypt.then(|| self.value(Key::DiskPassphrase).to_string()),
            },
            manual_disk: self.manual.as_ref().map(|m| m.config.clone()),
            uefi: self.uefi,
            hostname: v(Key::Hostname),
            username: user,
            password: self.value(Key::Password).to_string(),
            timezone: v(Key::Timezone),
            keyboard,
            locale: lang,
            hardware: self.hardware(),
        })
    }

    /// The packages the plan builds from the AUR (the helper first), from
    /// the selection as it stands and this machine's facts.
    pub fn aur_packages(&self) -> Result<Vec<String>, String> {
        let plan =
            plan::resolve(self.catalog, self.facts, &self.selection(), &|_| None).map_err(|e| format!("{e:#}"))?;
        let mut out: Vec<String> = Vec::new();
        for step in &plan.steps {
            let pkgs = match step {
                PlanStep::BootstrapAurHelper { package } => std::slice::from_ref(package),
                PlanStep::InstallAur { packages } => packages.as_slice(),
                _ => continue,
            };
            out.extend(pkgs.iter().filter(|p| !out.contains(p)).cloned().collect::<Vec<_>>());
        }
        Ok(out)
    }
}
