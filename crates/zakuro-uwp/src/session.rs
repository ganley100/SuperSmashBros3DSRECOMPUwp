//! starting the game, with the console settings for this app's region.

use std::path::Path;

use zakuro_core::{loader, Config, System};

use crate::region::Region;
use crate::scan::Found;

/// the console settings this app runs its game with.
pub fn config(region: &Region, data: &Path, found: &Found) -> Config {
    Config {
        region: region.console_region,
        language: region.language,
        data_dir: Some(data.to_path_buf()),
        // recompiled code is linked into the program when it was built
        // with some; an Xbox app can't load a library from a loose file
        linked: crate::linked::code(region),
        find_recompiled: false,
        // Vulkan and a C compiler aren't there on an Xbox: the software
        // renderer draws the 3D
        hardware_renderer: false,
        texture_packs: false,
        plugins_3gx_enabled: false,
        update: found.update.clone(),
        dlc: found.dlc.clone(),
        username: "Xbox".into(),
        ..Config::default()
    }
}

/// loads the game.
pub fn start(region: &Region, data: &Path, found: &Found) -> Result<System, String> {
    let config = config(region, data, found);
    loader::load(&found.game, config).map_err(|error| format!("Could not start {}:\n{error}", found.game.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{JPN, USA};

    #[test]
    fn each_region_runs_with_its_own_console_settings() {
        let found = Found { game: "game.3ds".into(), update: Some("update.cia".into()), dlc: vec!["dlc.cia".into()] };
        let usa = config(&USA, Path::new("usa"), &found);
        let jpn = config(&JPN, Path::new("jpn"), &found);
        assert_eq!((usa.region, usa.language), (1, 1));
        assert_eq!((jpn.region, jpn.language), (0, 0));
        assert_eq!(usa.data_dir.as_deref(), Some(Path::new("usa")));
        assert!(!usa.hardware_renderer);
        assert_eq!(usa.update, found.update);
        assert_eq!(usa.dlc, found.dlc);
    }
}
