//! Safe parsing and validation of Luma3DS/Citra 3GX plugin containers.
//!
//! This module deliberately does not execute plugins. Loading a 3GX file
//! also requires process-memory mapping, the plugin startup trampoline and
//! the plg:ldr HLE service; those belong to the runtime loader.

use std::ops::Range;

/// Size of the fixed 3GX header through the symbol-table descriptor.
pub const HEADER_SIZE: usize = 0x94;
/// The maximum size accepted for any one executable section.
pub const MAX_SECTION_SIZE: usize = 0x0500_0000;
/// Luma/Citra's NOP terminator for the embedded executable-check function.
pub const ARM_NOP: u32 = 0xE320_F000;
/// Address at which the Citra-compatible loader maps the plugin header.
pub const PLUGIN_HEADER_ADDRESS: u32 = 0x0700_0000;
/// Address immediately after the in-memory plugin header, where .text begins.
pub const PLUGIN_CODE_ADDRESS: u32 = PLUGIN_HEADER_ADDRESS + 0x100;
/// Address used by the 3GX loader for the plugin heap.
pub const PLUGIN_HEAP_ADDRESS: u32 = 0x0600_0000;

const PLUGIN_HEADER_BYTES: u64 = 0x100;
const BOOTLOADER_BYTES: u64 = 0x1000;
const PAGE_BYTES: u64 = 0x1000;

/// Which environments a 3GX plugin declares compatibility with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    /// Intended for an actual Nintendo 3DS console only.
    ConsoleOnly,
    /// Declares compatibility with Citra-style emulators.
    Emulator,
    /// Declares compatibility with both console and emulator.
    ConsoleAndEmulator,
    /// A reserved value from a newer or malformed plugin header.
    Reserved(u8),
}

impl Compatibility {
    fn from_flags(flags: u32) -> Self {
        match (flags >> 4) & 0x3 {
            0 => Self::ConsoleOnly,
            1 => Self::Emulator,
            2 => Self::ConsoleAndEmulator,
            value => Self::Reserved(value as u8),
        }
    }

    /// Whether the plugin explicitly declares emulator compatibility.
    pub fn supports_emulator(self) -> bool {
        matches!(self, Self::Emulator | Self::ConsoleAndEmulator)
    }
}

/// Validated feature bits from the 3GX information flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginFlags {
    pub embedded_executable_check: bool,
    pub embedded_swap_functions: bool,
    /// The index into the 3GX-defined memory-region size table.
    pub memory_region_index: u8,
    pub compatibility: Compatibility,
    pub events_self_managed: bool,
    pub swap_not_needed: bool,
    pub use_private_memory: bool,
    pub allow_homebrew_load: bool,
}

impl PluginFlags {
    fn parse(raw: u32) -> Self {
        Self {
            embedded_executable_check: raw & (1 << 0) != 0,
            embedded_swap_functions: raw & (1 << 1) != 0,
            memory_region_index: ((raw >> 2) & 0x3) as u8,
            compatibility: Compatibility::from_flags(raw),
            events_self_managed: raw & (1 << 6) != 0,
            swap_not_needed: raw & (1 << 7) != 0,
            use_private_memory: raw & (1 << 8) != 0,
            allow_homebrew_load: raw & (1 << 9) != 0,
        }
    }

    /// Size of the plugin memory block selected by the header.
    pub fn memory_region_size(&self) -> u32 {
        const MIB: u32 = 1024 * 1024;
        [5, 2, 10, 5][self.memory_region_index as usize] * MIB
    }
}

/// A 3GX container after all file offsets and section ranges have been
/// checked against the source bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin3gx {
    pub version: u32,
    pub author: String,
    pub title: String,
    pub summary: String,
    pub description: String,
    pub flags: PluginFlags,
    pub executable_checksum: u32,
    pub executable_check_args: [u32; 4],
    pub swap_load_args: [u32; 4],
    pub target_title_ids: Vec<u32>,
    pub code: Vec<u8>,
    pub rodata: Vec<u8>,
    pub data: Vec<u8>,
    pub bss_size: u32,
    /// Embedded ARM instructions used by the loader to check the base game's
    /// executable. None means the plugin expects the loader to supply one.
    pub executable_check: Option<Vec<u32>>,
    /// File ranges are retained for diagnostics and future memory mapping.
    pub code_file_range: Range<usize>,
    pub rodata_file_range: Range<usize>,
    pub data_file_range: Range<usize>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("the file is shorter than the fixed 3GX header")]
    TruncatedHeader,
    #[error("the file does not have the 3GX$0002 signature")]
    InvalidMagic,
    #[error("the {0} metadata string is larger than 0x400 bytes")]
    StringTooLarge(&'static str),
    #[error("the {0} metadata string points outside the plugin file")]
    InvalidStringRange(&'static str),
    #[error("the target-title table is too large")]
    TooManyTargets,
    #[error("the target-title table points outside the plugin file")]
    InvalidTargetTable,
    #[error("the {0} section is larger than the supported limit")]
    SectionTooLarge(&'static str),
    #[error("the {0} section points outside the plugin file")]
    InvalidSection(&'static str),
    #[error("the plugin has an empty code section")]
    EmptyCode,
    #[error("the {0} section size is not valid")]
    InvalidSize(&'static str),
    #[error("plugin executable and bootloader do not fit its declared memory region")]
    ExceedsMemoryRegion,
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let value: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(value))
}

fn read_string(
    bytes: &[u8],
    length_offset: usize,
    position_offset: usize,
    name: &'static str,
) -> Result<String, ParseError> {
    let length = read_u32(bytes, length_offset).ok_or(ParseError::TruncatedHeader)? as usize;
    let position = read_u32(bytes, position_offset).ok_or(ParseError::TruncatedHeader)? as usize;
    if length == 0 || position == 0 {
        return Ok(String::new());
    }
    if length > 0x400 {
        return Err(ParseError::StringTooLarge(name));
    }
    let end = position.checked_add(length).ok_or(ParseError::InvalidStringRange(name))?;
    let raw = bytes.get(position..end).ok_or(ParseError::InvalidStringRange(name))?;
    let raw = &raw[..raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len())];
    Ok(String::from_utf8_lossy(raw).into_owned())
}

fn read_section(
    bytes: &[u8],
    offset_field: usize,
    size_field: usize,
    name: &'static str,
) -> Result<(Vec<u8>, Range<usize>), ParseError> {
    let offset = read_u32(bytes, offset_field).ok_or(ParseError::TruncatedHeader)? as usize;
    let size = read_u32(bytes, size_field).ok_or(ParseError::TruncatedHeader)? as usize;
    if size > MAX_SECTION_SIZE {
        return Err(ParseError::SectionTooLarge(name));
    }
    if size == 0 {
        return Ok((Vec::new(), offset..offset));
    }
    let end = offset.checked_add(size).ok_or(ParseError::InvalidSection(name))?;
    let raw = bytes.get(offset..end).ok_or(ParseError::InvalidSection(name))?;
    Ok((raw.to_vec(), offset..end))
}

impl Plugin3gx {
    /// Parses a 3GX file and validates every variable-length range before
    /// retaining its sections. This does not map or execute guest code.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < HEADER_SIZE {
            return Err(ParseError::TruncatedHeader);
        }
        if bytes.get(0..8) != Some(b"3GX$0002".as_slice()) {
            return Err(ParseError::InvalidMagic);
        }

        let version = read_u32(bytes, 0x08).ok_or(ParseError::TruncatedHeader)?;
        let flags_raw = read_u32(bytes, 0x30).ok_or(ParseError::TruncatedHeader)?;
        let flags = PluginFlags::parse(flags_raw);

        let author = read_string(bytes, 0x10, 0x14, "author")?;
        let title = read_string(bytes, 0x18, 0x1C, "title")?;
        let summary = read_string(bytes, 0x20, 0x24, "summary")?;
        let description = read_string(bytes, 0x28, 0x2C, "description")?;

        let target_count = read_u32(bytes, 0x80).ok_or(ParseError::TruncatedHeader)? as usize;
        let target_offset = read_u32(bytes, 0x84).ok_or(ParseError::TruncatedHeader)? as usize;
        if target_count > 0x4000 {
            return Err(ParseError::TooManyTargets);
        }
        let target_title_ids = if target_count == 0 {
            Vec::new()
        } else {
            let target_bytes = target_count.checked_mul(4).ok_or(ParseError::TooManyTargets)?;
            let target_end = target_offset.checked_add(target_bytes).ok_or(ParseError::InvalidTargetTable)?;
            let target_data = bytes.get(target_offset..target_end).ok_or(ParseError::InvalidTargetTable)?;
            target_data
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
                .collect()
        };

        let (code, code_file_range) = read_section(bytes, 0x58, 0x64, "code")?;
        let (rodata, rodata_file_range) = read_section(bytes, 0x5C, 0x68, "rodata")?;
        let (data, data_file_range) = read_section(bytes, 0x60, 0x6C, "data")?;
        if code.is_empty() {
            return Err(ParseError::EmptyCode);
        }

        let bss_size = read_u32(bytes, 0x70).ok_or(ParseError::TruncatedHeader)?;
        if bss_size as usize > MAX_SECTION_SIZE {
            return Err(ParseError::InvalidSize("BSS"));
        }
        let exe_size = PLUGIN_HEADER_BYTES
            .checked_add(code.len() as u64)
            .and_then(|size| size.checked_add(rodata.len() as u64))
            .and_then(|size| size.checked_add(data.len() as u64))
            .and_then(|size| size.checked_add(u64::from(bss_size)))
            .and_then(|size| size.checked_add(PAGE_BYTES))
            .map(|size| size & !(PAGE_BYTES - 1))
            .ok_or(ParseError::ExceedsMemoryRegion)?;
        if exe_size + BOOTLOADER_BYTES > u64::from(flags.memory_region_size()) {
            return Err(ParseError::ExceedsMemoryRegion);
        }

        let executable_checksum = read_u32(bytes, 0x34).ok_or(ParseError::TruncatedHeader)?;
        let mut executable_check_args = [0; 4];
        let mut swap_load_args = [0; 4];
        for index in 0..4 {
            executable_check_args[index] =
                read_u32(bytes, 0x38 + index * 4).ok_or(ParseError::TruncatedHeader)?;
            swap_load_args[index] =
                read_u32(bytes, 0x48 + index * 4).ok_or(ParseError::TruncatedHeader)?;
        }

        let executable_check = if flags.embedded_executable_check {
            let offset = read_u32(bytes, 0x74).ok_or(ParseError::TruncatedHeader)? as usize;
            if offset == 0 {
                None
            } else {
                let mut instructions = Vec::new();
                for index in 0..32 {
                    let at = offset.checked_add(index * 4).ok_or(ParseError::InvalidSection("executable-check"))?;
                    let instruction = read_u32(bytes, at).ok_or(ParseError::InvalidSection("executable-check"))?;
                    if instruction == ARM_NOP {
                        break;
                    }
                    instructions.push(instruction);
                }
                (!instructions.is_empty()).then_some(instructions)
            }
        } else {
            None
        };

        Ok(Self {
            version,
            author,
            title,
            summary,
            description,
            flags,
            executable_checksum,
            executable_check_args,
            swap_load_args,
            target_title_ids,
            code,
            rodata,
            data,
            bss_size,
            executable_check,
            code_file_range,
            rodata_file_range,
            data_file_range,
        })
    }

    /// Whether the target-title table allows the supplied full 3DS title ID.
    /// The on-disk 3GX table stores the low 32 bits of each title ID.
    pub fn supports_title(&self, program_id: u64) -> bool {
        self.target_title_ids.is_empty()
            || self.target_title_ids.contains(&(program_id as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0xC4];
        bytes[0..8].copy_from_slice(b"3GX$0002");
        put_u32(&mut bytes, 0x08, 7);
        put_u32(&mut bytes, 0x10, 4);
        put_u32(&mut bytes, 0x14, 0xA0);
        put_u32(&mut bytes, 0x18, 10);
        put_u32(&mut bytes, 0x1C, 0xA4);
        put_u32(&mut bytes, 0x30, 1 << 4);
        put_u32(&mut bytes, 0x58, 0xC0);
        put_u32(&mut bytes, 0x64, 4);
        put_u32(&mut bytes, 0x80, 1);
        put_u32(&mut bytes, 0x84, HEADER_SIZE as u32);
        put_u32(&mut bytes, HEADER_SIZE, 0x0003_0600);
        bytes[0xA0..0xA4].copy_from_slice(b"Nick");
        bytes[0xA4..0xAE].copy_from_slice(b"MK7 Plugin");
        bytes[0xC0..0xC4].copy_from_slice(&[1, 2, 3, 4]);
        bytes
    }

    #[test]
    fn parses_sections_metadata_flags_and_targets() {
        let plugin = Plugin3gx::parse(&fixture()).unwrap();
        assert_eq!(plugin.version, 7);
        assert_eq!(plugin.author, "Nick");
        assert_eq!(plugin.title, "MK7 Plugin");
        assert_eq!(plugin.flags.compatibility, Compatibility::Emulator);
        assert_eq!(plugin.flags.memory_region_size(), 5 * 1024 * 1024);
        assert_eq!(plugin.code, [1, 2, 3, 4]);
        assert!(plugin.rodata.is_empty());
        assert!(plugin.data.is_empty());
        assert!(plugin.supports_title(0x0004_0000_0003_0600));
        assert!(!plugin.supports_title(0x0004_0000_0003_0700));
    }

    #[test]
    fn empty_target_table_means_all_titles() {
        let mut bytes = fixture();
        put_u32(&mut bytes, 0x80, 0);
        put_u32(&mut bytes, 0x84, u32::MAX);
        let plugin = Plugin3gx::parse(&bytes).unwrap();
        assert!(plugin.supports_title(0x0004_0000_0003_0600));
        assert!(plugin.supports_title(0x0004_0000_0003_0700));
    }

    #[test]
    fn rejects_bad_magic_and_truncated_sections() {
        let mut bytes = fixture();
        bytes[0] = b'X';
        assert_eq!(Plugin3gx::parse(&bytes), Err(ParseError::InvalidMagic));

        let mut bytes = fixture();
        put_u32(&mut bytes, 0x64, 0x1000);
        assert_eq!(Plugin3gx::parse(&bytes), Err(ParseError::InvalidSection("code")));
    }

    #[test]
    fn rejects_oversized_strings_and_sections() {
        let mut bytes = fixture();
        put_u32(&mut bytes, 0x10, 0x401);
        assert_eq!(Plugin3gx::parse(&bytes), Err(ParseError::StringTooLarge("author")));

        let mut bytes = fixture();
        put_u32(&mut bytes, 0x64, (MAX_SECTION_SIZE + 1) as u32);
        assert_eq!(Plugin3gx::parse(&bytes), Err(ParseError::SectionTooLarge("code")));
    }

    #[test]
    fn reads_embedded_executable_check_until_arm_nop() {
        let mut bytes = fixture();
        bytes.resize(0xD0, 0);
        put_u32(&mut bytes, 0x30, (1 << 4) | 1);
        put_u32(&mut bytes, 0x74, 0xC4);
        put_u32(&mut bytes, 0xC4, 0xE1A0_0000);
        put_u32(&mut bytes, 0xC8, 0xE1A0_1001);
        put_u32(&mut bytes, 0xCC, ARM_NOP);
        let plugin = Plugin3gx::parse(&bytes).unwrap();
        assert_eq!(plugin.executable_check, Some(vec![0xE1A0_0000, 0xE1A0_1001]));
    }

    #[test]
    fn rejects_executable_that_exceeds_plugin_memory() {
        let mut bytes = fixture();
        put_u32(&mut bytes, 0x30, (1 << 4) | (1 << 2)); // emulator, 2 MiB
        put_u32(&mut bytes, 0x64, 3 * 1024 * 1024);
        bytes.resize(0xC0 + 3 * 1024 * 1024, 0);
        assert_eq!(Plugin3gx::parse(&bytes), Err(ParseError::ExceedsMemoryRegion));
    }
}
