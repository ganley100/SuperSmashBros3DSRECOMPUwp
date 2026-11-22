//! CSND sound-channel HLE used by 3GX plugins.
//!
//! Models the 32-channel CSND PCM8/PCM16/IMA-ADPCM mixer and the csnd:SND
//! shared-memory command-list interface. PSG/noise generation and capture
//! DMA remain unsupported; capture registers and allocation commands are
//! retained so clients can initialize and query the service safely.

use std::collections::BTreeMap;

use crate::kernel::ipc::{CommandBuffer, Header};
use crate::kernel::object::{KObject, ObjectId, SharedMemory};
use crate::kernel::sync;
use crate::memory::{Memory, MemoryRegion, PhysicalBlock};
use crate::System;

const CHANNELS: usize = 32;
const CAPTURES: usize = 2;
const COMMAND_SIZE: u32 = 0x20;
const CHANNEL_STATE_SIZE: u32 = 0x0C;
const CAPTURE_STATE_SIZE: u32 = 8;
const CLIENT_CHANNELS: u32 = 0xFFFF_FF00;
const LIST_END: u16 = 0xFFFF;
const INVALID_STATE: u32 = 0xC8A0_B7FF;
const ALREADY_INITIALIZED: u32 = 0xE0A0_183B;
const INVALID_ADDRESS: u32 = 0xE0E3_FFF6;

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
struct Channel {
    cnt: u16,
    timer: u16,
    volume_left: u16,
    volume_right: u16,
    requested_left: u16,
    requested_right: u16,
    capture_volume_left: u16,
    capture_volume_right: u16,
    start: u32,
    size: u32,
    loop_address: u32,
    adpcm_state: u32,
    loop_adpcm_state: u32,
    block_address: u32,
    block_size: u32,
    position: u32,
    high_nibble: bool,
    holding: bool,
    counter: u32,
    previous: i16,
    current: i16,
    predictor: i16,
    step_index: u8,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Client {
    shared_block: Option<PhysicalBlock>,
    shared_object: Option<ObjectId>,
    mutex_object: Option<ObjectId>,
    shared_handle: u32,
    mutex_handle: u32,
    master_offset: u32,
    channel_offset: u32,
    capture_offset: u32,
    direct_offset: u32,
    channel_mask: u32,
    capture_mask: u8,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            shared_block: None, shared_object: None, mutex_object: None,
            shared_handle: 0, mutex_handle: 0,
            master_offset: 0, channel_offset: 0, capture_offset: 0, direct_offset: 0,
            channel_mask: 0, capture_mask: 0,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CsndState {
    channels: [Channel; CHANNELS],
    clients: BTreeMap<u32, Client>,
    next_session: u32,
    capture_in_use: [bool; CAPTURES],
    master_volume: u16,
}

impl Default for CsndState {
    fn default() -> Self {
        Self {
            channels: [Channel::default(); CHANNELS],
            clients: BTreeMap::new(),
            next_session: 1,
            capture_in_use: [false; CAPTURES],
            master_volume: 0x8000,
        }
    }
}

impl CsndState {
    pub fn open_session(&mut self) -> u32 {
        let id = self.next_session.max(1);
        self.next_session = id.wrapping_add(1).max(1);
        self.clients.entry(id).or_default();
        id
    }

    fn client(&mut self, session: u32) -> &mut Client {
        self.clients.entry(session).or_default()
    }

    fn start(&mut self, index: usize) {
        let channel = &mut self.channels[index];
        channel.block_address = channel.start;
        channel.block_size = channel.size;
        channel.position = 0;
        channel.high_nibble = false;
        channel.holding = false;
        channel.counter = 0;
        channel.previous = 0;
        channel.current = 0;
        channel.predictor = channel.adpcm_state as u16 as i16;
        channel.step_index = ((channel.adpcm_state >> 16) & 0x7F).min(88) as u8;
    }

    fn stop(&mut self, index: usize) {
        let channel = &mut self.channels[index];
        channel.adpcm_state = channel.predictor as u16 as u32 | ((channel.step_index as u32) << 16);
        channel.cnt &= !0x8000;
    }

    fn sample(&mut self, memory: &mut Memory, index: usize) -> i16 {
        const STEPS: [i32; 89] = [
            7,8,9,10,11,12,13,14,16,17,19,21,23,25,28,31,34,37,41,45,50,55,60,66,73,80,88,97,107,118,130,143,157,173,190,209,230,253,279,307,337,371,408,449,494,544,598,658,724,796,876,963,1060,1166,1282,1411,1552,1707,1878,2066,2272,2499,2749,3024,3327,3660,4026,4428,4871,5358,5894,6484,7132,7845,8630,9493,10442,11487,12635,13899,15289,16818,18500,20350,22385,24623,27086,29794,32767
        ];
        const INDEX: [i32; 8] = [-1,-1,-1,-1,2,4,6,8];
        let ch = &mut self.channels[index];
        let address = ch.block_address.wrapping_add(ch.position);
        let format = (ch.cnt >> 12) & 3;
        match format {
            0 => {
                let mut b = [0];
                let _ = memory.read_physical(address, &mut b);
                ch.position = ch.position.wrapping_add(1);
                (b[0] as i8 as i16) << 8
            }
            1 => {
                let mut b = [0; 2];
                let _ = memory.read_physical(address, &mut b);
                ch.position = ch.position.wrapping_add(2);
                i16::from_le_bytes(b)
            }
            2 => {
                let mut b = [0];
                let _ = memory.read_physical(address, &mut b);
                let nibble = if ch.high_nibble { b[0] >> 4 } else { b[0] & 0xF };
                if ch.high_nibble { ch.position = ch.position.wrapping_add(1); }
                ch.high_nibble = !ch.high_nibble;
                let step = STEPS[ch.step_index as usize];
                let mut diff = step >> 3;
                if nibble & 1 != 0 { diff += step >> 2; }
                if nibble & 2 != 0 { diff += step >> 1; }
                if nibble & 4 != 0 { diff += step; }
                let next = if nibble & 8 != 0 { ch.predictor as i32 - diff } else { ch.predictor as i32 + diff };
                ch.predictor = next.clamp(-0x7FFF, 0x7FFF) as i16;
                ch.step_index = (ch.step_index as i32 + INDEX[(nibble & 7) as usize]).clamp(0, 88) as u8;
                ch.predictor
            }
            _ => 0, // PSG/noise is not implemented by Azahar's software mixer either.
        }
    }

    fn step(&mut self, memory: &mut Memory, index: usize) {
        let (loop_mode, position, block_size) = {
            let channel = self.channels[index];
            ((channel.cnt >> 10) & 3, channel.position, channel.block_size)
        };

        if loop_mode != 0 && position >= block_size {
            if loop_mode == 2 {
                let hold = self.channels[index].cnt & 0x80 != 0;
                self.stop(index);
                self.channels[index].holding = hold;
                return;
            }

            {
                let channel = &mut self.channels[index];
                channel.block_address = channel.loop_address;
                if loop_mode == 1 {
                    channel.block_size = channel.size;
                }
                channel.position = 0;
                channel.high_nibble = false;
                if channel.loop_adpcm_state & 0x8000_0000 != 0 {
                    channel.predictor = channel.loop_adpcm_state as u16 as i16;
                    channel.step_index = ((channel.loop_adpcm_state >> 16) & 0x7F).min(88) as u8;
                }
            }

            if self.channels[index].block_size == 0 {
                self.stop(index);
                return;
            }
        }

        let current = self.sample(memory, index);
        let channel = &mut self.channels[index];
        channel.previous = channel.current;
        channel.current = current;
    }

    pub fn render_frame(&mut self, memory: &mut Memory, output: &mut Vec<[i16; 2]>) {
        if !self.clients.values().any(|client| client.shared_block.is_some())
            && !self.channels.iter().any(|channel| channel.cnt & 0x8000 != 0)
        {
            return;
        }

        let mut mixed = [[0i16; 2]; 160];
        for frame in &mut mixed {
            let mut left = 0i64;
            let mut right = 0i64;

            for index in 0..CHANNELS {
                let initial = self.channels[index];
                if initial.cnt & 0x8000 == 0 {
                    if initial.holding {
                        left += (initial.current as i64 * initial.volume_left as i64) >> 15;
                        right += (initial.current as i64 * initial.volume_right as i64) >> 15;
                    }
                    continue;
                }

                if initial.cnt & 0x4000 != 0 {
                    let period = (0x1_0000u32 - initial.timer as u32).max(0x42);
                    self.channels[index].counter += 2048;
                    while self.channels[index].counter >= period
                        && self.channels[index].cnt & 0x8000 != 0
                    {
                        self.channels[index].counter -= period;
                        self.step(memory, index);
                    }
                }

                let channel = self.channels[index];
                if channel.cnt & 0x8000 == 0 {
                    if channel.holding {
                        left += (channel.current as i64 * channel.volume_left as i64) >> 15;
                        right += (channel.current as i64 * channel.volume_right as i64) >> 15;
                    }
                    continue;
                }

                let sample = if channel.cnt & 0x40 != 0 {
                    let period = (0x1_0000u32 - channel.timer as u32).max(0x42) as i64;
                    let delta = channel.current as i64 - channel.previous as i64;
                    channel.previous as i64 + delta * channel.counter as i64 / period
                } else {
                    channel.current as i64
                };
                left += (sample * channel.volume_left as i64) >> 15;
                right += (sample * channel.volume_right as i64) >> 15;
            }

            frame[0] = ((left * self.master_volume as i64) >> 15).clamp(-32768, 32767) as i16;
            frame[1] = ((right * self.master_volume as i64) >> 15).clamp(-32768, 32767) as i16;
        }

        if output.len() < mixed.len() {
            output.resize(mixed.len(), [0, 0]);
        }
        let start = output.len() - mixed.len();
        for (dest, source) in output[start..].iter_mut().zip(mixed) {
            dest[0] = dest[0].saturating_add(source[0]);
            dest[1] = dest[1].saturating_add(source[1]);
        }
    }
}

pub fn advance(system: &mut System) {
    let dsp_running = system.services.dsp.running;
    let (csnd, dsp) = (&mut system.services.csnd, &mut system.services.dsp);
    let active = csnd.clients.values().any(|client| client.shared_block.is_some())
        || csnd.channels.iter().any(|channel| channel.cnt & 0x8000 != 0);
    if !dsp_running && active {
        dsp.output.extend(std::iter::repeat_n([0, 0], 160));
    }
    csnd.render_frame(&mut system.memory, &mut dsp.output);
    if dsp.output.len() > 32_768 {
        let excess = dsp.output.len() - 32_768;
        dsp.output.drain(..excess);
    }
}

pub fn handle(system: &mut System, buffer: &CommandBuffer, header: Header, session: u32) -> bool {
    let id = header.command_id();
    match id {
        0x0001 => initialize(system, buffer, id, session),
        0x0002 => { shutdown(system, session); buffer.reply(&mut system.memory, id, &[]); true }
        0x0003 => execute_commands(system, buffer, id, session),
        0x0005 => {
            let client = system.services.csnd.client(session);
            client.channel_mask = CLIENT_CHANNELS;
            buffer.reply(&mut system.memory, id, &[CLIENT_CHANNELS]);
            true
        }
        0x0006 => {
            system.services.csnd.client(session).channel_mask = 0;
            buffer.reply(&mut system.memory, id, &[]);
            true
        }
        0x0007 => {
            let unit = system.services.csnd.capture_in_use.iter().position(|used| !used);
            if let Some(unit) = unit {
                system.services.csnd.capture_in_use[unit] = true;
                system.services.csnd.client(session).capture_mask |= 1 << unit;
                let client = system.services.csnd.client(session).clone();
                write_shared_u32(system, &client, client.capture_offset + unit as u32 * CAPTURE_STATE_SIZE + 4, 0);
                buffer.reply(&mut system.memory, id, &[unit as u32]);
            } else {
                buffer.reply_error(&mut system.memory, id, INVALID_STATE);
            }
            true
        }
        0x0008 => {
            let unit = buffer.get(&mut system.memory, 1) as usize;
            if unit >= CAPTURES || system.services.csnd.client(session).capture_mask & (1 << unit) == 0 {
                buffer.reply_error(&mut system.memory, id, INVALID_STATE);
            } else {
                system.services.csnd.client(session).capture_mask &= !(1 << unit);
                system.services.csnd.capture_in_use[unit] = false;
                buffer.reply(&mut system.memory, id, &[]);
            }
            true
        }
        0x0009..=0x000B => {
            let address = buffer.get(&mut system.memory, 1);
            let size = buffer.get(&mut system.memory, 2);
            let end = address.checked_add(size);
            let valid = end.is_some_and(|end| {
                (0x1400_0000..=0x1C00_0000).contains(&address) && end <= 0x1C00_0000
                    || (0x3000_0000..=0x4000_0000).contains(&address) && end <= 0x4000_0000
            });
            if valid { buffer.reply(&mut system.memory, id, &[]); }
            else { buffer.reply_error(&mut system.memory, id, INVALID_ADDRESS); }
            true
        }
        0x000C => {
            for index in 0..CHANNELS {
                system.services.csnd.stop(index);
                let ch = &mut system.services.csnd.channels[index];
                ch.volume_left = 0;
                ch.volume_right = 0;
            }
            buffer.reply(&mut system.memory, id, &[]);
            true
        }
        _ => false,
    }
}

fn initialize(system: &mut System, buffer: &CommandBuffer, id: u16, session: u32) -> bool {
    let size = buffer.get(&mut system.memory, 1).max(0x1000).saturating_add(0xFFF) & !0xFFF;
    let client = system.services.csnd.client(session);
    if client.shared_block.is_some() {
        buffer.reply_error(&mut system.memory, id, ALREADY_INITIALIZED);
        return true;
    }
    let Some(block) = system.memory.phys.allocate(MemoryRegion::Base, size) else {
        buffer.reply_error(&mut system.memory, id, zakuro_common::result::errors::OUT_OF_MEMORY.0);
        return true;
    };
    system.memory.zero_physical(block.addr, block.size);
    let master_offset = buffer.get(&mut system.memory, 2);
    let channel_offset = buffer.get(&mut system.memory, 3);
    let capture_offset = buffer.get(&mut system.memory, 4);
    let direct_offset = buffer.get(&mut system.memory, 5);

    let mutex_object = system.kernel.objects.insert(KObject::Mutex(sync::Mutex::new("CSND")));
    let mutex_handle = system.kernel.handles.create(&mut system.kernel.objects, mutex_object, "csnd:SND mutex");
    system.kernel.objects.add_ref(mutex_object);
    let shared_object = system.kernel.objects.insert(KObject::SharedMemory(SharedMemory {
        name: "CSND".into(), address: 0, size, paddr: block.addr, mapped_at: None,
    }));
    let shared_handle = system.kernel.handles.create(&mut system.kernel.objects, shared_object, "csnd:SND shared memory");
    system.kernel.objects.add_ref(shared_object);
    {
        let client = system.services.csnd.client(session);
        client.shared_block = Some(block);
        client.shared_object = Some(shared_object);
        client.mutex_object = Some(mutex_object);
        client.shared_handle = shared_handle;
        client.mutex_handle = mutex_handle;
        client.master_offset = master_offset;
        client.channel_offset = channel_offset;
        client.capture_offset = capture_offset;
        client.direct_offset = direct_offset;
    }
    buffer.set(&mut system.memory, 0, Header::new(id, 1, 3).0);
    buffer.set(&mut system.memory, 1, 0);
    buffer.set(&mut system.memory, 2, crate::kernel::ipc::Descriptor::move_handles(2));
    buffer.set(&mut system.memory, 3, mutex_handle);
    buffer.set(&mut system.memory, 4, shared_handle);
    true
}

fn shutdown(system: &mut System, session: u32) {
    let Some(mut client) = system.services.csnd.clients.remove(&session) else { return; };
    if let Some(object) = client.shared_object {
        if let Some(KObject::SharedMemory(shared)) = system.kernel.objects.get(object) {
            if let Some(address) = shared.mapped_at { system.memory.unmap(address, shared.size); }
        }
        system.kernel.handles.close(&mut system.kernel.objects, client.shared_handle);
        system.kernel.objects.release(object);
    }
    if let Some(object) = client.mutex_object {
        system.kernel.handles.close(&mut system.kernel.objects, client.mutex_handle);
        system.kernel.objects.release(object);
    }
    if let Some(block) = client.shared_block.take() { system.memory.phys.free(block); }
    for unit in 0..CAPTURES {
        if client.capture_mask & (1 << unit) != 0 { system.services.csnd.capture_in_use[unit] = false; }
    }
}

/// Releases the per-session CSND resources when its service handle is closed.
pub fn close_session(system: &mut System, session: u32) {
    shutdown(system, session);
}

/// Releases every CSND client when the emulated process exits.
pub fn shutdown_all(system: &mut System) {
    let sessions: Vec<u32> = system.services.csnd.clients.keys().copied().collect();
    for session in sessions {
        shutdown(system, session);
    }
}

fn execute_commands(system: &mut System, buffer: &CommandBuffer, id: u16, session: u32) -> bool {
    let first = buffer.get(&mut system.memory, 1);
    let client = system.services.csnd.client(session).clone();
    let Some(block) = client.shared_block else {
        buffer.reply_error(&mut system.memory, id, INVALID_STATE);
        return true;
    };
    let mut offset = first;
    let mut executed = 0u32;
    let limit = block.size / COMMAND_SIZE;
    while offset != LIST_END as u32 && executed < limit {
        if offset.checked_add(COMMAND_SIZE).is_none_or(|end| end > block.size) { break; }
        let mut command = [0u8; COMMAND_SIZE as usize];
        if !system.memory.read_physical(block.addr + offset, &mut command) { break; }
        let next = u16::from_le_bytes([command[0], command[1]]);
        let command_id = u16::from_le_bytes([command[2], command[3]]);
        let mut params = [0u32; 6];
        for (index, param) in params.iter_mut().enumerate() {
            let at = 8 + index * 4;
            *param = u32::from_le_bytes(command[at..at + 4].try_into().unwrap());
        }
        execute_command(system, &client, command_id, params);
        offset = next as u32;
        executed += 1;
    }
    if first.checked_add(COMMAND_SIZE).is_some_and(|end| end <= block.size) {
        let _ = system.memory.write_physical(block.addr + first + 4, &[1]);
    }
    buffer.reply(&mut system.memory, id, &[]);
    true
}

fn execute_command(system: &mut System, client: &Client, command: u16, p: [u32; 6]) {
    let index = (p[0] & 0x1F) as usize;
    if command < 0x100 && client.channel_mask & (1 << index) == 0 {
        return;
    }
    if (0x100..0x200).contains(&command) && index >= CAPTURES {
        return;
    }
    if command == 0x300 {
        write_client_state(system, client);
        return;
    }

    // CSND capture DMA and PSG/noise are not implemented by the software
    // channel model. Their commands remain recognized rather than falling
    // through to the generic unimplemented-service path.
    if (0x100..=0x106).contains(&command)
        || command == 0x200
        || (0x00F..=0x010).contains(&command)
    {
        return;
    }

    match command {
        // SetPlayStateRaw
        0x000 => {
            if p[1] != 0 {
                if system.services.csnd.channels[index].cnt & 0x8000 == 0 {
                    system.services.csnd.start(index);
                }
                system.services.csnd.channels[index].cnt |= 0x8000;
            } else {
                system.services.csnd.stop(index);
                let channel = &mut system.services.csnd.channels[index];
                channel.holding = false;
                channel.volume_left = 0;
                channel.volume_right = 0;
            }
            return;
        }
        // SetPlayState
        0x001 => {
            if p[1] != 0 {
                if system.services.csnd.channels[index].cnt & 0x8000 == 0 {
                    system.services.csnd.start(index);
                }
                system.services.csnd.channels[index].volume_left = system.services.csnd.channels[index].requested_left;
                system.services.csnd.channels[index].volume_right = system.services.csnd.channels[index].requested_right;
                system.services.csnd.channels[index].cnt |= 0xC000;
            } else {
                // The hardware fades before pausing. This implementation
                // applies the terminal silent/pause state immediately.
                let channel = &mut system.services.csnd.channels[index];
                channel.cnt &= !0x4000;
                channel.volume_left = 0;
                channel.volume_right = 0;
            }
            return;
        }
        // ConfigureChannel
        0x00E => {
            let flags = p[0];
            {
                let channel = &mut system.services.csnd.channels[index];
                channel.cnt = 0;
                channel.timer = (0x1_0000u32.wrapping_sub(flags >> 16)) as u16;
                channel.volume_left = 0;
                channel.volume_right = 0;
                channel.capture_volume_left = p[2] as u16;
                channel.capture_volume_right = (p[2] >> 16) as u16;
                channel.loop_address = p[4];
                channel.size = p[5] & 0x07FF_FFFF;
                channel.start = p[3];
                if ((flags >> 12) & 3) != 2 {
                    channel.adpcm_state = 0;
                    channel.loop_adpcm_state = 0;
                }
                channel.cnt = (((flags >> 12) & 3) as u16) << 12
                    | (((flags >> 10) & 3) as u16) << 10
                    | if flags & (1 << 6) != 0 { 0x40 } else { 0 };
                channel.requested_left = (p[1] as u16).min(0x8000);
                channel.requested_right = ((p[1] >> 16) as u16).min(0x8000);
                channel.volume_left = 0;
                channel.volume_right = 0;
            }
            if flags & (1 << 14) != 0 {
                system.services.csnd.start(index);
                let channel = &mut system.services.csnd.channels[index];
                channel.volume_left = channel.requested_left;
                channel.volume_right = channel.requested_right;
                channel.cnt |= 0xC000;
            }
            write_client_state(system, client);
            return;
        }
        _ => {}
    }

    let busy = system.services.csnd.channels[index].cnt & 0x8000 != 0;
    {
        let channel = &mut system.services.csnd.channels[index];
        match command {
            0x002 if !busy => {
                channel.cnt = (channel.cnt & !0x3000) | (((p[1] & 0xFF) as u16) << 12);
            }
            0x003 => {
                channel.loop_address = p[1];
                channel.size = p[2] & 0x07FF_FFFF;
            }
            0x004 if !busy => {
                channel.cnt = (channel.cnt & !0x0C00) | (((p[1] & 3) as u16) << 10);
            }
            0x005 => {
                if p[1] != 0 { channel.cnt |= 0x80; } else { channel.cnt &= !0x80; }
            }
            0x006 if !busy => {
                if p[1] != 0 { channel.cnt |= 0x40; } else { channel.cnt &= !0x40; }
            }
            0x008 => channel.timer = (0x1_0000u32.wrapping_sub(p[1])) as u16,
            0x009 => {
                channel.requested_left = (p[1] as u16).min(0x8000);
                channel.requested_right = ((p[1] >> 16) as u16).min(0x8000);
                channel.volume_left = channel.requested_left;
                channel.volume_right = channel.requested_right;
                channel.capture_volume_left = p[2] as u16;
                channel.capture_volume_right = (p[2] >> 16) as u16;
            }
            // SetBlock changes source range and size; decoder state is retained.
            0x00A => {
                channel.start = p[1];
                channel.size = p[2] & 0x07FF_FFFF;
            }
            0x00B => {
                channel.adpcm_state = (p[1] & 0xFFFF) | ((p[2] & 0xFF) << 16);
                channel.loop_adpcm_state = 0;
            }
            0x00C => {
                channel.loop_adpcm_state =
                    (p[1] & 0xFFFF) | ((p[2] & 0xFF) << 16) | 0x8000_0000;
            }
            0x00D => {
                if p[1] == 0 { channel.loop_adpcm_state &= !0x8000_0000; }
                else { channel.loop_adpcm_state |= 0x8000_0000; }
            }
            _ => {}
        }
    }
    if command == 0x00A {
        write_client_state(system, client);
    }
}

fn write_shared(system: &mut System, client: &Client, offset: u32, bytes: &[u8]) {
    let Some(block) = client.shared_block else { return; };
    if offset.checked_add(bytes.len() as u32).is_some_and(|end| end <= block.size) {
        let _ = system.memory.write_physical(block.addr + offset, bytes);
    }
}

fn write_shared_u32(system: &mut System, client: &Client, offset: u32, value: u32) {
    write_shared(system, client, offset, &value.to_le_bytes());
}

fn write_client_state(system: &mut System, client: &Client) {
    let mask = client.channel_mask;
    let active_mask = (0..CHANNELS).fold(0u32, |bits, i| {
        if mask & (1 << i) != 0 && system.services.csnd.channels[i].cnt & 0xC000 == 0xC000 { bits | (1 << i) } else { bits }
    });
    write_shared_u32(system, client, client.master_offset, active_mask);
    write_shared_u32(system, client, client.master_offset + 4, client.capture_mask as u32);
    let mut position = 0u32;
    for index in 0..CHANNELS {
        if mask & (1 << index) == 0 { continue; }
        let ch = system.services.csnd.channels[index];
        let at = client.channel_offset + position * CHANNEL_STATE_SIZE;
        write_shared(system, client, at, &[u8::from(ch.cnt & 0xC000 == 0xC000)]);
        write_shared(system, client, at + 4, &(ch.predictor as u16).to_le_bytes());
        write_shared(system, client, at + 6, &[ch.step_index]);
        write_shared_u32(system, client, at + 8, 0);
        position += 1;
    }
    for unit in 0..CAPTURES {
        if client.capture_mask & (1 << unit) == 0 { continue; }
        write_shared(system, client, client.capture_offset + unit as u32 * CAPTURE_STATE_SIZE, &[0]);
        write_shared_u32(system, client, client.capture_offset + unit as u32 * CAPTURE_STATE_SIZE + 4, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_csnd_session_gets_independent_state() {
        let mut csnd = CsndState::default();
        let first = csnd.open_session();
        let second = csnd.open_session();
        assert_ne!(first, second);
        assert!(csnd.clients.contains_key(&first));
        assert!(csnd.clients.contains_key(&second));
    }

    #[test]
    fn configure_channel_preserves_requested_volume_across_pause_and_resume() {
        let mut system = System::new(crate::Config::default());
        let mut client = Client::default();
        client.channel_mask = 1;

        execute_command(
            &mut system,
            &client,
            0x00E,
            [0, 0x4000_3000, 0, 0, 0x1000, 0x100],
        );
        assert_eq!(system.services.csnd.channels[0].requested_left, 0x3000);
        assert_eq!(system.services.csnd.channels[0].requested_right, 0x4000);

        execute_command(&mut system, &client, 0x001, [0, 0, 0, 0, 0, 0]);
        assert_eq!(system.services.csnd.channels[0].volume_left, 0);
        assert_eq!(system.services.csnd.channels[0].volume_right, 0);

        execute_command(&mut system, &client, 0x001, [0, 1, 0, 0, 0, 0]);
        assert_eq!(system.services.csnd.channels[0].volume_left, 0x3000);
        assert_eq!(system.services.csnd.channels[0].volume_right, 0x4000);

        execute_command(&mut system, &client, 0x000, [0, 0, 0, 0, 0, 0]);
        assert_eq!(system.services.csnd.channels[0].volume_left, 0);
        assert_eq!(system.services.csnd.channels[0].volume_right, 0);
        execute_command(&mut system, &client, 0x000, [0, 1, 0, 0, 0, 0]);
        assert_eq!(system.services.csnd.channels[0].volume_left, 0);
        assert_eq!(system.services.csnd.channels[0].volume_right, 0);
    }

    #[test]
    fn pcm8_channel_is_mixed_into_the_audio_frame() {
        let mut memory = Memory::new(false, 64 * 1024 * 1024);
        let block = memory.phys.allocate(MemoryRegion::Base, 0x1000).unwrap();
        memory.write_physical(block.addr, &[0x40; 64]);
        let mut csnd = CsndState::default();
        let channel = &mut csnd.channels[0];
        channel.start = block.addr;
        channel.size = 64;
        channel.timer = 1;
        channel.volume_left = 0x8000;
        channel.volume_right = 0x8000;
        channel.cnt = 0xC800; // enabled, running, one-shot loop mode, PCM8
        csnd.start(0);
        let mut output = vec![[0i16; 2]; 160];
        csnd.render_frame(&mut memory, &mut output);
        assert!(output.iter().any(|sample| sample[0] > 0 && sample[1] > 0));
    }
}
