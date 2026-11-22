//! plg:ldr, the Luma3DS/Azahar 3GX plugin-loader service.
//!
//! Implements the public IPC surface used by 3GX launchers. Actual plugin
//! installation is performed before the title's main thread starts by
//! plugin_3gx_loader.

use crate::kernel::ipc::{CommandBuffer, Descriptor, Header};
use crate::services::Target;
use crate::System;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PluginLoaderState {
    pub enabled: bool,
    pub allow_game_change: bool,
    pub plugin_loaded: bool,
    pub is_default_path: bool,
    pub plugin_path: String,
    pub requested_path: String,
    pub plugin_process_id: u32,
    pub memory_region: u8,
    pub memory_block: Option<crate::memory::PhysicalBlock>,
    pub plugin_entry: u32,
    pub exe_size: u32,
    pub heap_size: u32,
    pub use_user_load_parameters: bool,
    pub no_flash: bool,
    pub memory_strategy: u8,
    pub low_title_id: u32,
    pub config: [u32; 32],
    /// The game's two original instructions, restored when its process exits.
    pub original_game_entry: Option<[u32; 2]>,
    /// Original CONFIG_MEM app_mem_alloc value, restored on plugin unload.
    pub previous_app_memory_size: Option<u32>,
}

impl Default for PluginLoaderState {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_game_change: true,
            plugin_loaded: false,
            is_default_path: false,
            plugin_path: String::new(),
            requested_path: String::new(),
            plugin_process_id: u32::MAX,
            memory_region: 0,
            memory_block: None,
            plugin_entry: 0,
            exe_size: 0,
            heap_size: 0,
            use_user_load_parameters: false,
            no_flash: false,
            memory_strategy: 0,
            low_title_id: 0,
            config: [0; 32],
            original_game_entry: None,
            previous_app_memory_size: None,
        }
    }
}

fn mapped_buffers(system: &mut System, buffer: &CommandBuffer, header: Header) -> Vec<(u32, u32, u32)> {
    let mut found = Vec::new();
    let mut index = 1 + header.normal_params();
    let end = index + header.translate_params();
    while index < end {
        let descriptor = buffer.get(&mut system.memory, index);
        index += 1;
        match Descriptor::parse(descriptor) {
            Descriptor::MappedBuffer { size, .. } => {
                if index >= end { break; }
                let pointer = buffer.get(&mut system.memory, index);
                index += 1;
                found.push((descriptor, pointer, size));
            }
            Descriptor::Handles { count, .. } => index = index.saturating_add(count).min(end),
            Descriptor::StaticBuffer { .. } => index = index.saturating_add(1).min(end),
            Descriptor::CurrentProcessId | Descriptor::Unknown(_) => {}
        }
    }
    found
}

fn read_mapped_string(system: &mut System, pointer: u32, size: u32, limit: usize) -> String {
    let length = (size as usize).min(limit);
    let mut bytes = vec![0; length];
    system.memory.read_bytes(pointer, &mut bytes);
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn handle(system: &mut System, buffer: &CommandBuffer, header: Header) -> bool {
    let command = header.command_id();
    match command {
        // LoadPlugin is intentionally not a late-load operation: the bootloader
        // must be installed before the main thread enters game code.
        0x0001 => {
            buffer.reply_error(&mut system.memory, command, zakuro_common::result::errors::UNIMPLEMENTED.0);
            true
        }
        // IsEnabled
        0x0002 => {
            buffer.reply(&mut system.memory, command, &[system.services.plgldr.enabled as u32]);
            true
        }
        // SetEnabled
        0x0003 => {
            let enabled = buffer.get(&mut system.memory, 1) == 1;
            let can_change = enabled == system.services.plgldr.enabled || system.services.plgldr.allow_game_change;
            if can_change {
                system.services.plgldr.enabled = enabled;
                buffer.reply(&mut system.memory, command, &[]);
            } else {
                buffer.reply_error(&mut system.memory, command, zakuro_common::result::errors::UNIMPLEMENTED.0);
            }
            true
        }
        // SetLoadSettings(flags, low_title_id, path mapped buffer, config mapped buffer)
        0x0004 => {
            let flags = buffer.get(&mut system.memory, 1);
            system.services.plgldr.no_flash = flags & 1 != 0;
            system.services.plgldr.memory_strategy = ((flags >> 8) & 0xF) as u8;
            system.services.plgldr.low_title_id = buffer.get(&mut system.memory, 2);
            let mapped = mapped_buffers(system, buffer, header);
            if let Some((_, pointer, size)) = mapped.first().copied() {
                system.services.plgldr.requested_path = read_mapped_string(system, pointer, size, 255);
            }
            if let Some((_, pointer, size)) = mapped.get(1).copied() {
                let count = (size as usize / 4).min(32);
                let mut bytes = vec![0; count * 4];
                system.memory.read_bytes(pointer, &mut bytes);
                for (index, word) in bytes.chunks_exact(4).enumerate() {
                    system.services.plgldr.config[index] = u32::from_le_bytes(word.try_into().unwrap());
                }
            }
            system.services.plgldr.use_user_load_parameters = true;
            buffer.reply(&mut system.memory, command, &[]);
            true
        }
        // DisplayMenu / DisplayMessage are UI owned by the host.
        0x0005 | 0x0006 => {
            buffer.reply(&mut system.memory, command, &[]);
            true
        }
        // DisplayErrorMessage(error, title, description)
        0x0007 => {
            let error = buffer.get(&mut system.memory, 1);
            let mapped = mapped_buffers(system, buffer, header);
            let title = mapped.first().map(|(_, pointer, size)| read_mapped_string(system, *pointer, *size, 0x400)).unwrap_or_default();
            let description = mapped.get(1).map(|(_, pointer, size)| read_mapped_string(system, *pointer, *size, 0x1000)).unwrap_or_default();
            log::error!("3GX plugin error 0x{error:08X}: {title}: {description}");
            buffer.reply(&mut system.memory, command, &[]);
            true
        }
        // GetPLGLDRVersion, CoreVersion(1, 0, 2)
        0x0008 => {
            buffer.reply(&mut system.memory, command, &[0x0100_0002]);
            true
        }
        // GetArbiter is not meaningful for this HLE service.
        0x0009 => {
            buffer.reply_error(&mut system.memory, command, zakuro_common::result::errors::UNIMPLEMENTED.0);
            true
        }
        // GetPluginPath writes the path to the caller's mapped buffer.
        0x000A => {
            let mapped = mapped_buffers(system, buffer, header);
            if let Some((descriptor, pointer, size)) = mapped.first().copied() {
                let path = if !system.services.plgldr.plugin_path.is_empty() {
                    let plugin_path = std::path::Path::new(&system.services.plgldr.plugin_path);
                    // The service reports paths relative to SDMC, not to the
                    // application's host data directory (whose SD root is
                    // the "user" subdirectory).
                    let sdmc_root = system.services.fs.user_dir.join("sdmc");
                    plugin_path
                        .strip_prefix(&sdmc_root)
                        .ok()
                        .or_else(|| {
                            system.config.data_dir.as_deref()
                                .and_then(|root| plugin_path.strip_prefix(root).ok())
                        })
                        .map(|p| format!("/{}", p.to_string_lossy().replace('\\', "/")))
                        .unwrap_or_else(|| system.services.plgldr.plugin_path.clone())
                } else {
                    system.services.plgldr.requested_path.clone()
                };
                let mut bytes = path.into_bytes();
                bytes.push(0);
                bytes.truncate(size as usize);
                system.memory.write_bytes(pointer, &bytes);
                buffer.set(&mut system.memory, 0, Header::new(command, 1, 2).0);
                buffer.set(&mut system.memory, 1, 0);
                buffer.set(&mut system.memory, 2, descriptor);
                buffer.set(&mut system.memory, 3, pointer);
            } else {
                buffer.reply(&mut system.memory, command, &[]);
            }
            true
        }
        // Rosalina menu block and swap/load-exe parameters are optional hooks
        // not required by the standard 3GX startup trampoline.
        0x000B..=0x000D => {
            buffer.reply(&mut system.memory, command, &[]);
            true
        }
        _ => false,
    }
}

#[allow(dead_code)]
pub fn install_interfaces(_system: &mut System) {
    // svcConnectToPort and srv:GetServiceHandle are name-based in this HLE
    // runtime; routing is registered in services::handle_request.
}

#[allow(dead_code)]
fn _target_type(_: Target) {}
