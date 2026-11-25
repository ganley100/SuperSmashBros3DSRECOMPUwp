//! which release of the game this app is. the USA and Japan releases are
//! separate apps, each with its own package identity, title ID, console
//! settings and data folder, so both can be installed on one Xbox.

#[cfg(all(feature = "usa", feature = "jpn"))]
compile_error!("build one region at a time: --no-default-features --features jpn for Japan, the default for USA");

/// one regional release of Super Smash Bros. for Nintendo 3DS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// a short code, used in folder and package names.
    pub code: &'static str,
    /// the name a person reads.
    pub name: &'static str,
    /// the base game's title ID.
    pub program_id: u64,
    /// the folder under D:\DevelopmentFiles this app keeps its files in.
    pub folder: &'static str,
    /// the console's region, as cfg:u reports it.
    pub console_region: u8,
    /// the console's language, as cfg:u reports it.
    pub language: u8,
}

pub const USA: Region = Region {
    code: "USA",
    name: "USA",
    program_id: 0x0004_0000_000E_DF00,
    folder: "SmashBros3DS-USA",
    console_region: zakuro_core::services::cfg::REGION_USA,
    language: zakuro_core::services::cfg::LANGUAGE_ENGLISH,
};

pub const JPN: Region = Region {
    code: "JPN",
    name: "Japan",
    program_id: 0x0004_0000_000B_8B00,
    folder: "SmashBros3DS-JPN",
    console_region: zakuro_core::services::cfg::REGION_JAPAN,
    // the console's Japanese, which cfg:u numbers 0
    language: 0,
};

/// the release this app was built for.
#[cfg(feature = "jpn")]
pub const THIS: Region = JPN;
#[cfg(not(feature = "jpn"))]
pub const THIS: Region = USA;

impl Region {
    /// whether a title, the game, its update or its DLC, is this release's.
    /// the three share the low half of their title IDs.
    pub fn owns(&self, title_id: u64) -> bool {
        title_id & 0xFFFF_FFFF == self.program_id & 0xFFFF_FFFF
    }

    /// the other release's name, when a title is the game from another
    /// region, for telling the player which app to use.
    pub fn other_release(title_id: u64) -> Option<&'static str> {
        match title_id {
            id if id == USA.program_id => Some("USA"),
            id if id == JPN.program_id => Some("Japan"),
            0x0004_0000_000E_E000 => Some("Europe/Australia"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_release_owns_its_update_and_dlc_and_not_the_others() {
        assert!(USA.owns(0x0004_0000_000E_DF00));
        assert!(USA.owns(0x0004_000E_000E_DF00));
        assert!(USA.owns(0x0004_008C_000E_DF00));
        assert!(!USA.owns(0x0004_0000_000B_8B00));
        assert!(JPN.owns(0x0004_0000_000B_8B00));
        assert!(JPN.owns(0x0004_000E_000B_8B00));
        assert!(JPN.owns(0x0004_008C_000B_8B00));
        assert!(!JPN.owns(0x0004_0000_000E_DF00));
    }

    #[test]
    fn the_two_apps_keep_their_files_apart() {
        assert_ne!(USA.folder, JPN.folder);
        assert_ne!(USA.console_region, JPN.console_region);
        assert_ne!(USA.language, JPN.language);
    }
}
