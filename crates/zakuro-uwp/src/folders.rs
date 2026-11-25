//! where the app keeps its files. on an Xbox in Dev Mode that is
//! D:\DevelopmentFiles\SmashBros3DS-<region>, which the Device Portal's file
//! explorer can write to from a PC, falling back to the app's own LocalState
//! folder when the console doesn't let the app in there.

use std::path::{Path, PathBuf};

/// the folder Xbox Dev Mode's Device Portal shares for files, the default
/// home of everything this app reads and writes.
pub const DEVELOPMENT_FILES: &str = r"D:\DevelopmentFiles";

/// the folders the app works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folders {
    /// saves, logs and everything else the app writes, and where the game
    /// dump goes.
    pub data: PathBuf,
    /// more folders to look in for the game, after data.
    pub search: Vec<PathBuf>,
    /// why data isn't the preferred folder, when it isn't.
    pub note: Option<String>,
}

/// the app's own folder under D:\DevelopmentFiles.
pub fn preferred(folder: &str) -> PathBuf {
    Path::new(DEVELOPMENT_FILES).join(folder)
}

/// the preferred folder when the app can write there, else the fallback,
/// the package's LocalState.
pub fn choose(preferred: PathBuf, fallback: Option<PathBuf>) -> Folders {
    let parent = preferred.parent().map(Path::to_path_buf);
    match writable(&preferred) {
        Ok(()) => Folders { data: preferred, search: parent.into_iter().collect(), note: None },
        Err(error) => match fallback {
            Some(fallback) => {
                let note = format!(
                    "{} could not be used ({error}), so files are kept in {} instead.",
                    preferred.display(),
                    fallback.display()
                );
                // the dump may still be readable where it was meant to go
                let search = [Some(preferred), parent].into_iter().flatten().collect();
                Folders { data: fallback, search, note: Some(note) }
            }
            None => Folders { data: preferred, search: parent.into_iter().collect(), note: Some(format!("{error}")) },
        },
    }
}

/// makes the folder when it isn't there, and checks a file can be written in
/// it.
fn writable(folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let probe = folder.join(".write-test");
    std::fs::write(&probe, b"ok")?;
    std::fs::remove_file(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_folder_is_under_development_files() {
        assert_eq!(preferred("SmashBros3DS-USA"), Path::new(r"D:\DevelopmentFiles").join("SmashBros3DS-USA"));
    }

    #[test]
    fn a_writable_folder_is_used_and_its_parent_searched() {
        let root = std::env::temp_dir().join(format!("smash-uwp-folders-{}", std::process::id()));
        let wanted = root.join("SmashBros3DS-USA");
        let folders = choose(wanted.clone(), None);
        assert_eq!(folders.data, wanted);
        assert_eq!(folders.search, vec![root.clone()]);
        assert!(folders.note.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unwritable_folder_falls_back_and_says_why() {
        let root = std::env::temp_dir().join(format!("smash-uwp-fallback-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        // a file where the folder should be can't be made into one
        let blocked = root.join("blocked");
        std::fs::write(&blocked, b"").unwrap();
        let wanted = blocked.join("SmashBros3DS-JPN");
        let fallback = root.join("LocalState");
        let folders = choose(wanted.clone(), Some(fallback.clone()));
        assert_eq!(folders.data, fallback);
        assert!(folders.search.contains(&wanted));
        assert!(folders.note.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }
}
