//! # neko-audio
//!
//! The audio output layer, after Godot's `AudioDriver`
//! (servers/audio/audio_server.h). A backend owns the device and its thread;
//! whenever the device needs samples it calls [`Mixer::mix`], like Godot's
//! drivers call `audio_server_process`. Everything else (buses, effects,
//! streams) is platform-independent engine code.
//!
//! Porting audio to a new platform means implementing [`AudioBackend`]: open
//! the device, call the mixer from its callback, stop it.

/// Speaker layout (Godot's `AudioServer::SpeakerMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeakerMode {
    Stereo,
    Surround31,
    Surround51,
    Surround71,
}

impl SpeakerMode {
    pub fn channels(self) -> usize {
        match self {
            Self::Stereo => 2,
            Self::Surround31 => 4,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }
}

/// What the backend actually opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioConfig {
    /// Frames per second (Godot's `mix_rate`, 44100 or 48000 typically).
    pub mix_rate: u32,
    pub speakers: SpeakerMode,
    /// Frames per callback, if the device reports it.
    pub buffer_frames: Option<u32>,
}

/// What the engine asks for; backends pick the closest the device supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioRequest {
    pub mix_rate: u32,
    pub speakers: SpeakerMode,
    /// Desired output latency in milliseconds (`audio/driver/output_latency`, 15 in Godot).
    pub latency_ms: u32,
}

impl Default for AudioRequest {
    fn default() -> Self {
        Self { mix_rate: 44_100, speakers: SpeakerMode::Stereo, latency_ms: 15 }
    }
}

/// The engine side: produces interleaved f32 frames in -1..1.
///
/// Called on the backend's audio thread; it must not block.
pub trait Mixer: Send {
    /// Fill `out` (interleaved, `config.speakers.channels()` per frame).
    fn mix(&mut self, out: &mut [f32]);
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio device available: {0}")]
    NoDevice(String),
    #[error("audio backend failed: {0}")]
    Backend(String),
}

/// An audio output backend.
pub trait AudioBackend {
    /// Short name, e.g. `"sdl3"`, `"cpal"`, `"dummy"`.
    fn name(&self) -> &'static str;
    /// Open the device and start pulling from `mixer`.
    fn start(&mut self, request: AudioRequest, mixer: Box<dyn Mixer>) -> Result<AudioConfig, AudioError>;
    /// Stop the device and drop the mixer.
    fn stop(&mut self);
}

/// A backend with no device: keeps the engine running where there is no
/// sound (servers, tests, platforms not ported yet). Godot's `AudioDriverDummy`.
#[derive(Default)]
pub struct DummyAudio {
    mixer: Option<Box<dyn Mixer>>,
}

impl DummyAudio {
    /// Advance the mixer by `frames` without playing them (headless runs).
    pub fn pump(&mut self, frames: usize) {
        if let Some(m) = &mut self.mixer {
            let mut buf = vec![0.0; frames * 2];
            m.mix(&mut buf);
        }
    }
}

impl AudioBackend for DummyAudio {
    fn name(&self) -> &'static str {
        "dummy"
    }
    fn start(&mut self, request: AudioRequest, mixer: Box<dyn Mixer>) -> Result<AudioConfig, AudioError> {
        self.mixer = Some(mixer);
        Ok(AudioConfig { mix_rate: request.mix_rate, speakers: SpeakerMode::Stereo, buffer_frames: None })
    }
    fn stop(&mut self) {
        self.mixer = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Count(Arc<AtomicUsize>);
    impl Mixer for Count {
        fn mix(&mut self, out: &mut [f32]) {
            out.fill(0.0);
            self.0.fetch_add(out.len(), Ordering::Relaxed);
        }
    }

    #[test]
    fn dummy_pulls_from_the_mixer() {
        let n = Arc::new(AtomicUsize::new(0));
        let mut a = DummyAudio::default();
        let cfg = a.start(AudioRequest::default(), Box::new(Count(n.clone()))).unwrap();
        assert_eq!(cfg.mix_rate, 44_100);
        a.pump(512);
        assert_eq!(n.load(Ordering::Relaxed), 1024);
        a.stop();
        a.pump(512);
        assert_eq!(n.load(Ordering::Relaxed), 1024);
    }
}
