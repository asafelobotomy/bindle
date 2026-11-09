//! Bindle's icon, as installed by `first-login install`: the SVG for desktops
//! that scale it, and PNGs at the hicolor sizes for those that don't.
//! The PNGs come from `scripts/icons.sh`; rerun it after editing the SVG.

/// The scalable icon; also what `first-login status` checks for.
pub const SVG_PATH: &str = "/usr/local/share/icons/hicolor/scalable/apps/bindle.svg";
const SVG: &[u8] = include_bytes!("../../../branding/bindle.svg");

macro_rules! pngs {
    ($($n:literal),*) => {
        [$(($n, include_bytes!(concat!("../../../branding/png/bindle-", $n, ".png")) as &[u8])),*]
    };
}
const PNGS: [(u32, &[u8]); 9] = pngs!(16, 22, 24, 32, 48, 64, 128, 256, 512);

/// Every icon file: (path, contents).
pub fn files() -> Vec<(String, &'static [u8])> {
    let mut out = vec![(SVG_PATH.to_string(), SVG)];
    out.extend(PNGS.iter().map(|&(n, data)| (format!("/usr/local/share/icons/hicolor/{n}x{n}/apps/bindle.png"), data)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Width and height from a PNG's IHDR chunk.
    fn png_size(data: &[u8]) -> (u32, u32) {
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
        let be = |at: usize| u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
        (be(16), be(20))
    }

    #[test]
    fn every_png_has_its_size() {
        for (n, data) in PNGS {
            assert_eq!(png_size(data), (n, n), "bindle-{n}.png");
        }
        assert_eq!(files().len(), 1 + PNGS.len());
    }

    #[test]
    fn icons_match_the_svg() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let out =
            std::process::Command::new("sha256sum").arg("branding/bindle.svg").current_dir(root).output().unwrap();
        let sum = String::from_utf8(out.stdout).unwrap();
        let recorded = std::fs::read_to_string(format!("{root}/branding/png/SOURCE.sha256")).unwrap();
        assert_eq!(
            sum.split_whitespace().next(),
            Some(recorded.trim()),
            "branding/bindle.svg changed: run scripts/icons.sh to re-render the PNGs"
        );
    }
}
