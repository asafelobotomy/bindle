//! Unified kernel images (UKIs) on the EFI partition: which kernel each one
//! holds. A UKI is a PE file whose `.uname` section is the kernel's release
//! string, so it is read straight from the section table (no objcopy, and
//! never the whole 60+ MB file).

use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

/// Where UKIs are looked for, as the boot partition is mounted on Arch.
pub const DIRS: &[&str] = &["/boot/EFI/Linux", "/efi/EFI/Linux", "/boot/efi/EFI/Linux"];

/// A `.efi` file in one of [`DIRS`] and the kernel it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Uki {
    /// Absolute path, for messages.
    pub path: String,
    /// The `.uname` section (`7.3.5-zen1-1-zen`); empty when there is none.
    pub kernel: String,
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// The `.uname` section of a PE image; None when it isn't a PE file or has
/// no such section.
pub fn uname<R: Read + Seek>(r: &mut R) -> Option<String> {
    let mut dos = [0u8; 0x40];
    r.read_exact(&mut dos).ok()?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    r.seek(SeekFrom::Start(u32_at(&dos, 0x3c)? as u64)).ok()?;
    let mut coff = [0u8; 24];
    r.read_exact(&mut coff).ok()?;
    if &coff[..4] != b"PE\0\0" {
        return None;
    }
    let sections = u16_at(&coff, 6)?;
    let optional = u16_at(&coff, 20)?;
    r.seek(SeekFrom::Current(optional as i64)).ok()?;
    for _ in 0..sections.min(96) {
        let mut s = [0u8; 40];
        r.read_exact(&mut s).ok()?;
        let name = s[..8].split(|&c| c == 0).next().unwrap_or_default();
        if name != b".uname" {
            continue;
        }
        let (size, offset) = (u32_at(&s, 16)?, u32_at(&s, 20)?);
        if size == 0 || size > 4096 {
            return None;
        }
        r.seek(SeekFrom::Start(offset as u64)).ok()?;
        let mut text = vec![0u8; size];
        r.read_exact(&mut text).ok()?;
        let text = String::from_utf8_lossy(&text);
        return Some(text.trim_matches(['\0', '\n', ' ']).to_string());
    }
    None
}

/// The UKIs under `root` (the real "/" or a test directory), sorted by path.
/// None when the boot partition can't be read (it is often root-only), which
/// is not the same as having no UKIs.
pub fn scan(root: &Path) -> Option<Vec<Uki>> {
    let mut found = vec![];
    let (mut seen_dir, mut unreadable) = (false, false);
    for dir in DIRS {
        let host = root.join(dir.trim_start_matches('/'));
        let entries = match fs::read_dir(&host) {
            Ok(e) => e,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(_) => {
                unreadable = true;
                continue;
            }
        };
        seen_dir = true;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| !x.eq_ignore_ascii_case("efi")) {
                continue;
            }
            let Ok(mut f) = File::open(&path) else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            found.push(Uki { path: format!("{dir}/{name}"), kernel: uname(&mut f).unwrap_or_default() });
        }
    }
    if !seen_dir && unreadable {
        return None;
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup();
    Some(found)
}

/// What is wrong with the UKIs, given the installed kernel versions.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Findings {
    /// UKIs holding a kernel that isn't installed (or none that can be
    /// read): the boot menu can start them, without their modules.
    pub stale: Vec<Uki>,
    /// Installed kernels no UKI holds: the next boot can't use them.
    pub missing: Vec<String>,
}

/// With no UKIs at all the system boots some other way: nothing is wrong.
pub fn check(installed: &[String], ukis: &[Uki]) -> Findings {
    if ukis.is_empty() {
        return Findings::default();
    }
    Findings {
        stale: ukis.iter().filter(|u| !installed.contains(&u.kernel)).cloned().collect(),
        missing: installed.iter().filter(|v| !ukis.iter().any(|u| &u.kernel == *v)).cloned().collect(),
    }
}

/// A minimal UKI for tests and rehearsals: a PE header with one `.uname`
/// section.
pub fn fake_image(kernel: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x40];
    b[..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    b.extend_from_slice(b"PE\0\0");
    let mut coff = [0u8; 20];
    coff[2..4].copy_from_slice(&2u16.to_le_bytes()); // sections
    coff[16..18].copy_from_slice(&8u16.to_le_bytes()); // optional header size
    b.extend_from_slice(&coff);
    b.extend_from_slice(&[0u8; 8]);
    let data_at = (b.len() + 80) as u32;
    let mut section = |name: &[u8], size: u32, at: u32| {
        let mut s = [0u8; 40];
        s[..name.len()].copy_from_slice(name);
        s[16..20].copy_from_slice(&size.to_le_bytes());
        s[20..24].copy_from_slice(&at.to_le_bytes());
        b.extend_from_slice(&s);
    };
    section(b".osrel", 4, data_at + kernel.len() as u32 + 1);
    section(b".uname", kernel.len() as u32 + 1, data_at);
    b.extend_from_slice(kernel.as_bytes());
    b.push(0);
    b.extend_from_slice(b"ID=a");
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn uki(path: &str, kernel: &str) -> Uki {
        Uki { path: path.into(), kernel: kernel.into() }
    }

    #[test]
    fn reads_the_uname_section() {
        assert_eq!(uname(&mut Cursor::new(fake_image("7.3.5-zen1-1-zen"))).as_deref(), Some("7.3.5-zen1-1-zen"));
        assert_eq!(uname(&mut Cursor::new(b"not an image".to_vec())), None);
        assert_eq!(uname(&mut Cursor::new(Vec::new())), None);
        let mut cut = fake_image("7.3.5-zen1-1-zen");
        cut.truncate(0x40 + 30);
        assert_eq!(uname(&mut Cursor::new(cut)), None);
    }

    #[test]
    fn scans_the_boot_partition() {
        let root = std::env::temp_dir().join(format!("bindle-uki-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(scan(&root), Some(vec![]), "no UKI directory: not a UKI system");
        let dir = root.join("boot/EFI/Linux");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("arch-linux-zen.efi"), fake_image("7.3.5-zen1-1-zen")).unwrap();
        fs::write(dir.join("arch-linux-zen-previous.efi"), fake_image("7.3.4-zen2-1-zen")).unwrap();
        fs::write(dir.join("broken.efi"), b"junk").unwrap();
        fs::write(dir.join("readme.txt"), b"x").unwrap();
        let found = scan(&root).unwrap();
        let kernels: Vec<_> = found.iter().map(|u| (u.path.as_str(), u.kernel.as_str())).collect();
        assert_eq!(
            kernels,
            [
                ("/boot/EFI/Linux/arch-linux-zen-previous.efi", "7.3.4-zen2-1-zen"),
                ("/boot/EFI/Linux/arch-linux-zen.efi", "7.3.5-zen1-1-zen"),
                ("/boot/EFI/Linux/broken.efi", ""),
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stale_image_and_a_missing_one() {
        let installed = vec!["7.3.5-zen1-1-zen".to_string()];
        let current = uki("/boot/EFI/Linux/a.efi", "7.3.5-zen1-1-zen");
        let old = uki("/boot/EFI/Linux/a-previous.efi", "7.3.4-zen2-1-zen");
        assert_eq!(check(&installed, std::slice::from_ref(&current)), Findings::default());
        assert_eq!(check(&installed, &[current.clone(), old.clone()]).stale, std::slice::from_ref(&old));
        let f = check(&installed, std::slice::from_ref(&old));
        assert_eq!((f.stale, f.missing), (vec![old], vec!["7.3.5-zen1-1-zen".to_string()]));
        assert_eq!(check(&installed, &[]), Findings::default(), "no UKIs: boots another way");
        assert_eq!(check(&installed, &[current, uki("/x.efi", "")]).stale.len(), 1, "unreadable counts as stale");
    }
}
