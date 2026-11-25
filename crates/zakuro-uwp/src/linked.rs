//! recompiled code linked into the app.
//!
//! an Xbox app can't run a C compiler or load a library from a loose file,
//! so the code `3dsrecomp port` makes from your own dump, on a PC, is linked
//! into the app when it is built: set SMASH3DS_RECOMP_DIR to the port's
//! `code` folder, the one holding `librecomp.a`. without it the game runs in
//! the interpreter. see docs/XBOX.md.

use zakuro_core::recompiled::Linked;

use crate::region::Region;

#[cfg(smash_linked)]
mod tables {
    // the tables 3dsrecomp writes beside the code build.rs links in
    unsafe extern "C" {
        pub static recomp_abi: u32;
        pub static recomp_generation: u32;
        pub static recomp_entry_count: u32;
        pub static recomp_entries: u8;
        pub static recomp_module_count: u32;
        pub static recomp_modules: u8;
        pub static recomp_origins: u8;
    }
}

/// the linked code, when the app was built with some.
#[cfg(smash_linked)]
pub fn code(region: &Region) -> Option<Linked> {
    use std::ffi::c_void;
    use std::ptr::addr_of;
    use tables::*;
    // SAFETY: the symbols are 3dsrecomp's tables for one program, linked in
    // by build.rs, and live as long as the program
    let linked = unsafe {
        Linked::new(
            region.program_id,
            recomp_abi,
            addr_of!(recomp_entries) as *const c_void,
            recomp_entry_count,
            addr_of!(recomp_modules) as *const c_void,
            recomp_module_count,
        )
        .with_origins(recomp_generation, addr_of!(recomp_origins) as *const c_void)
    };
    Some(linked)
}

#[cfg(not(smash_linked))]
pub fn code(_region: &Region) -> Option<Linked> {
    None
}

/// whether the app was built with recompiled code, for the status line.
pub const LINKED: bool = cfg!(smash_linked);
