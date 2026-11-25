//! the console's sound through XAudio2, which every UWP app on an Xbox has.

use std::collections::VecDeque;

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::XAudio2::{
    IXAudio2, IXAudio2MasteringVoice, IXAudio2SourceVoice, XAudio2CreateWithVersionInfo, XAUDIO2_BUFFER, XAUDIO2_DEFAULT_CHANNELS,
    XAUDIO2_DEFAULT_SAMPLERATE, XAUDIO2_VOICE_STATE,
};
use windows::Win32::Media::Audio::{AudioCategory_GameEffects, WAVEFORMATEX, WAVE_FORMAT_PCM};

/// buffers queued at most, a frame's sound each, about 130 ms. more than
/// that and the sound has fallen behind the game, and new ones are dropped.
const MOST_QUEUED: usize = 8;
/// XAUDIO2_DEFAULT_PROCESSOR
const DEFAULT_PROCESSOR: u32 = 0x0000_0001;
/// NTDDI_WIN10
const WINDOWS_10: u32 = 0x0A00_0000;

pub struct Audio {
    // the engine and master voice outlive the source voice
    _engine: IXAudio2,
    _master: IXAudio2MasteringVoice,
    voice: IXAudio2SourceVoice,
    /// the samples of the buffers XAudio2 has, which it reads from until it
    /// has played them, oldest first.
    queued: VecDeque<Vec<i16>>,
}

impl Audio {
    pub fn open(sample_rate: f64) -> windows::core::Result<Audio> {
        unsafe {
            let mut engine = None;
            XAudio2CreateWithVersionInfo(&mut engine, 0, DEFAULT_PROCESSOR, WINDOWS_10)?;
            let engine = engine.ok_or_else(windows::core::Error::empty)?;
            let mut master = None;
            engine.CreateMasteringVoice(
                &mut master,
                XAUDIO2_DEFAULT_CHANNELS,
                XAUDIO2_DEFAULT_SAMPLERATE,
                0,
                PCWSTR::null(),
                None,
                AudioCategory_GameEffects,
            )?;
            let master = master.ok_or_else(windows::core::Error::empty)?;
            let rate = sample_rate.round() as u32;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 2,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * 4,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let mut voice = None;
            engine.CreateSourceVoice(&mut voice, &format, 0, 2.0, None, None, None)?;
            let voice = voice.ok_or_else(windows::core::Error::empty)?;
            voice.Start(0, 0)?;
            Ok(Audio { _engine: engine, _master: master, voice, queued: VecDeque::new() })
        }
    }

    /// plays a frame's samples after those already queued.
    pub fn play(&mut self, samples: &[[i16; 2]]) {
        self.forget_played();
        if samples.is_empty() || self.queued.len() >= MOST_QUEUED {
            return;
        }
        let data: Vec<i16> = samples.iter().flatten().copied().collect();
        let buffer = XAUDIO2_BUFFER {
            AudioBytes: (data.len() * 2) as u32,
            pAudioData: data.as_ptr().cast(),
            ..Default::default()
        };
        // SAFETY: the samples stay in queued, unmoved on the heap, until
        // XAudio2 says it has played them
        if unsafe { self.voice.SubmitSourceBuffer(&buffer, None) }.is_ok() {
            self.queued.push_back(data);
        }
    }

    /// drops the buffers XAudio2 is done with, the oldest ones.
    fn forget_played(&mut self) {
        let mut state = XAUDIO2_VOICE_STATE::default();
        unsafe { self.voice.GetState(&mut state, 0) };
        while self.queued.len() > state.BuffersQueued as usize {
            self.queued.pop_front();
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        // nothing may be read from queued once it goes
        unsafe { self.voice.DestroyVoice() };
    }
}
