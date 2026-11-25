//! finds this region's game dump, and its newest update and its DLC, among
//! the files in a folder.

use std::path::{Path, PathBuf};

use crate::region::Region;

/// file endings of the dumps the runtime opens.
const GAME_FILES: [&str; 4] = ["3ds", "cci", "cxi", "cia"];

/// the game found and what goes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub game: PathBuf,
    pub update: Option<PathBuf>,
    pub dlc: Vec<PathBuf>,
}

/// what was seen in the folders when the game wasn't found, to tell the
/// player what is wrong.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Missing {
    /// the folders looked in.
    pub searched: Vec<PathBuf>,
    /// dumps of the game from another region, and which.
    pub other_regions: Vec<(PathBuf, &'static str)>,
    /// this region's game, but encrypted.
    pub encrypted: Vec<PathBuf>,
}

impl Missing {
    /// what to tell the player.
    pub fn explain(&self, region: &Region) -> String {
        let mut text = format!(
            "No decrypted Super Smash Bros. for Nintendo 3DS ({}) dump was found.\n\nCopy your own dump (.3ds, .cci, .cxi or .cia), and any matching update and DLC CIA files, into:\n",
            region.name
        );
        for folder in &self.searched {
            text.push_str(&format!("    {}\n", folder.display()));
        }
        text.push_str("\nUse the Xbox Device Portal's File explorer (DevelopmentFiles) from a PC, then restart the app.");
        for path in &self.encrypted {
            text.push_str(&format!("\n\n{} is encrypted. Decrypt it on a PC first.", file_name(path)));
        }
        for (path, other) in &self.other_regions {
            text.push_str(&format!(
                "\n\n{} is the {other} release. This app runs the {} release only; install the {other} app for it.",
                file_name(path),
                region.name
            ));
        }
        text
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// looks through each folder in turn, the files directly in it, for the
/// region's game, and takes the update and DLC from the folder it is in.
pub fn find(region: &Region, folders: &[PathBuf]) -> Result<Found, Missing> {
    let mut missing = Missing { searched: folders.to_vec(), ..Missing::default() };
    for folder in folders {
        let files = files_in(folder);
        let mut game = None;
        let mut update: Option<(u16, PathBuf)> = None;
        let mut dlc = Vec::new();
        for path in files {
            // a file that trips a reader bug is skipped, not the whole scan
            let Ok(kind) = std::panic::catch_unwind(|| classify(&path)) else {
                log::warn!("reading {} failed", path.display());
                continue;
            };
            match kind {
                Some(File::Game(id)) if id == region.program_id => {
                    game.get_or_insert(path);
                }
                Some(File::Game(id)) => {
                    if let Some(other) = Region::other_release(id) {
                        missing.other_regions.push((path, other));
                    }
                }
                Some(File::Encrypted(id)) if region.owns(id) => missing.encrypted.push(path),
                Some(File::Update(id, version)) if region.owns(id) => {
                    if update.as_ref().is_none_or(|(newest, _)| version > *newest) {
                        update = Some((version, path));
                    }
                }
                Some(File::Dlc(id)) if region.owns(id) => dlc.push(path),
                _ => {}
            }
        }
        if let Some(game) = game {
            return Ok(Found { game, update: update.map(|(_, path)| path), dlc });
        }
    }
    Err(missing)
}

/// the files directly in a folder that may be dumps, in name order.
fn files_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| GAME_FILES.iter().any(|known| known.eq_ignore_ascii_case(extension)))
        })
        .collect();
    files.sort();
    files
}

enum File {
    /// a game that can be played, by title ID.
    Game(u64),
    /// a game in a CIA whose contents are encrypted.
    Encrypted(u64),
    Update(u64, u16),
    Dlc(u64),
}

fn classify(path: &Path) -> Option<File> {
    let is_cia = path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("cia"));
    if is_cia {
        let image = zakuro_fs::RomImage::open(path).ok()?;
        let cia = zakuro_fs::Cia::read(image.data()).ok()?;
        let encrypted = cia.contents.iter().any(|content| content.offset.is_some() && content.encrypted());
        match cia.kind() {
            zakuro_fs::TitleKind::Game if encrypted => return Some(File::Encrypted(cia.title_id)),
            zakuro_fs::TitleKind::Game => {}
            zakuro_fs::TitleKind::Update if !encrypted => return Some(File::Update(cia.title_id, cia.version)),
            zakuro_fs::TitleKind::Dlc if !encrypted => return Some(File::Dlc(cia.title_id)),
            _ => return None,
        }
    }
    let title = zakuro_fs::Title::load(path).ok()?;
    Some(File::Game(title.program_id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{JPN, USA};
    use zakuro_fs::testing::{cia, ncch};

    fn folder(test: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("smash-uwp-scan-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn each_app_finds_only_its_own_region_and_its_addons() {
        let dir = folder("regions");
        let usa = USA.program_id;
        let jpn = JPN.program_id;
        std::fs::write(dir.join("smash-usa.cxi"), ncch(usa, Some(&[1; 8]), &[])).unwrap();
        std::fs::write(dir.join("smash-jpn.cxi"), ncch(jpn, Some(&[1; 8]), &[])).unwrap();
        for (name, id, version) in [("usa-v1.cia", usa, 1u16), ("usa-v3.cia", usa, 3), ("jpn-v2.cia", jpn, 2)] {
            let update = id | 0x0000_000E_0000_0000;
            std::fs::write(dir.join(name), cia(update, version, &[(0, Some(&ncch(update, Some(&[2; 8]), &[])))])).unwrap();
        }

        let found = find(&USA, std::slice::from_ref(&dir)).expect("the USA game");
        assert_eq!(found.game, dir.join("smash-usa.cxi"));
        assert_eq!(found.update, Some(dir.join("usa-v3.cia")), "the newest update");

        let found = find(&JPN, std::slice::from_ref(&dir)).expect("the Japanese game");
        assert_eq!(found.game, dir.join("smash-jpn.cxi"));
        assert_eq!(found.update, Some(dir.join("jpn-v2.cia")));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn another_regions_dump_is_named_when_this_ones_is_missing() {
        let dir = folder("other");
        std::fs::write(dir.join("smash-jpn.cxi"), ncch(JPN.program_id, Some(&[1; 8]), &[])).unwrap();
        let missing = find(&USA, std::slice::from_ref(&dir)).expect_err("no USA game");
        assert_eq!(missing.other_regions, vec![(dir.join("smash-jpn.cxi"), "Japan")]);
        assert!(missing.explain(&USA).contains("install the Japan app"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn folders_are_searched_in_order() {
        let first = folder("first");
        let second = folder("second");
        std::fs::write(second.join("smash.cxi"), ncch(USA.program_id, Some(&[1; 8]), &[])).unwrap();
        let found = find(&USA, &[first.clone(), second.clone()]).expect("found in the second folder");
        assert_eq!(found.game, second.join("smash.cxi"));
        std::fs::remove_dir_all(first).unwrap();
        std::fs::remove_dir_all(second).unwrap();
    }
}
