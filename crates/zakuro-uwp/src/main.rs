//! Super Smash Bros. for Nintendo 3DS, recompiled, as a UWP app for an Xbox
//! One or Series console in Dev Mode. the USA and Japan releases are built
//! as two apps; see region.rs and docs/XBOX.md.

#![cfg_attr(windows, windows_subsystem = "windows")]
// the Windows parts use what the other platforms leave unused
#![cfg_attr(not(windows), allow(dead_code))]

mod controls;
mod folders;
mod layout;
mod linked;
mod region;
mod scan;
mod session;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod graphics;
#[cfg(windows)]
mod logfile;

#[cfg(windows)]
fn main() {
    if let Err(error) = app::run() {
        log::error!("the app stopped: {error}");
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "smash-3ds-uwp is the Xbox (UWP) app for the {} release; build it for x86_64-uwp-windows-msvc, see docs/XBOX.md",
        region::THIS.name
    );
    std::process::exit(2);
}
