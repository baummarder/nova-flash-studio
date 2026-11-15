//! Audio playback for the game preview.
//!
//! Wraps a rodio output stream: every [`play`] call decodes the embedded
//! asset bytes (WAV/MP3/FLAC/OGG via symphonia) into its own [`Player`], so
//! multiple sounds can overlap. Volume is a master gain applied per player;
//! pause/resume/stop map onto the rodio player controls.
//!
//! Limitations (documented, not hidden):
//! - Playback requires an audio output device. Headless environments get a
//!   construction error surfaced through [`AudioEngine::last_error`].
//! - `SoundCue.loop_count` is honoured by the SWF exporter; the preview
//!   plays each cue once per timeline-frame entry.
//! - `SoundCue.offset`/`duration` trimming is not applied in the preview.

use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};

use nova_core::Asset;

static NEXT_SOUND_ID: AtomicU64 = AtomicU64::new(1);

struct ActiveSound {
    player: rodio::Player,
}

/// Owns the output stream and all currently playing preview sounds.
pub struct AudioEngine {
    mixer: Option<rodio::mixer::Mixer>,
    sounds: Vec<ActiveSound>,
    /// Master volume 0.0..=1.0 applied to every player.
    volume: f32,
    /// Last construction/playback error for UI surfacing.
    pub last_error: Option<String>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    /// Opens the default audio output device. Never fails hard: on error the
    /// engine stays inert and `last_error` explains why.
    pub fn new() -> Self {
        let mut engine = Self {
            mixer: None,
            sounds: Vec::new(),
            volume: 1.0,
            last_error: None,
        };
        match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => engine.mixer = Some(sink.mixer().clone()),
            Err(error) => {
                engine.last_error = Some(format!("Kein Audio-Ausgangsgerät: {error}"));
            }
        }
        engine
    }

    /// Decodes `asset.data` and starts playback. Returns the sound handle on
    /// success; incompatible/missing data yields an error string.
    pub fn play(&mut self, asset: &Asset) -> Result<u64, String> {
        let Some(mixer) = &self.mixer else {
            return Err(self
                .last_error
                .clone()
                .unwrap_or_else(|| "Kein Audio-Ausgangsgerät".into()));
        };
        if asset.data.is_empty() {
            return Err(format!("Asset '{}' hat keine Audiodaten", asset.name));
        }
        let decoder = rodio::Decoder::new(Cursor::new(asset.data.clone()))
            .map_err(|error| format!("{}: {error}", asset.name))?;
        let player = rodio::Player::connect_new(mixer);
        player.set_volume(self.volume);
        player.append(decoder);
        let id = NEXT_SOUND_ID.fetch_add(1, Ordering::Relaxed);
        self.sounds.push(ActiveSound { player });
        self.prune();
        Ok(id)
    }

    /// Stops all preview sounds.
    pub fn stop_all(&mut self) {
        for sound in self.sounds.iter_mut() {
            sound.player.stop();
        }
        self.sounds.clear();
    }

    /// Master volume 0.0..=1.0 for all current and future sounds.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        for sound in self.sounds.iter_mut() {
            sound.player.set_volume(self.volume);
        }
    }

    /// Pauses every playing sound (rodio keeps buffered audio).
    pub fn pause_all(&mut self) {
        for sound in self.sounds.iter_mut() {
            sound.player.pause();
        }
    }

    /// Resumes paused sounds.
    pub fn resume_all(&mut self) {
        for sound in self.sounds.iter_mut() {
            sound.player.play();
        }
    }

    /// Drops finished players so `sounds` cannot grow unboundedly.
    pub fn prune(&mut self) {
        self.sounds.retain(|sound| !sound.player.empty());
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Collects the sound cues scheduled on `frame` of `scene`: every sound
/// layer's `frame.sound` entry whose asset exists in `assets`.
pub fn cues_on_frame<'a>(
    scene: &'a nova_core::Scene,
    frame: usize,
    assets: &'a [Asset],
) -> Vec<(&'a Asset, nova_core::SoundCue)> {
    let mut cues = Vec::new();
    for layer in &scene.layers {
        if layer.kind != nova_core::LayerKind::Sound {
            continue;
        }
        let Some(cue) = layer.frames.get(frame).and_then(|f| f.sound) else {
            continue;
        };
        if let Some(asset) = assets.iter().find(|asset| asset.id == cue.asset) {
            cues.push((asset, cue));
        }
    }
    cues
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_core::model::{Layer, LayerKind, Scene, SoundCue};
    use uuid::Uuid;

    /// Minimal valid PCM WAV: 8 kHz mono 16-bit, 8 samples of silence.
    fn tiny_wav() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&52u32.to_le_bytes()); // RIFF size = file - 8
        data.extend_from_slice(b"WAVEfmt ");
        data.extend_from_slice(&16u32.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes()); // PCM
        data.extend_from_slice(&1u16.to_le_bytes()); // mono
        data.extend_from_slice(&8000u32.to_le_bytes());
        data.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
        data.extend_from_slice(&2u16.to_le_bytes()); // block align
        data.extend_from_slice(&16u16.to_le_bytes()); // bits
        data.extend_from_slice(b"data");
        data.extend_from_slice(&16u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 16]);
        data
    }

    #[test]
    fn wav_asset_decodes_through_rodio() {
        let cursor = Cursor::new(tiny_wav());
        let decoder = rodio::Decoder::new(cursor);
        assert!(decoder.is_ok(), "valid WAV must decode");
    }

    #[test]
    fn invalid_audio_bytes_are_rejected() {
        let cursor = Cursor::new(b"not audio at all".to_vec());
        assert!(rodio::Decoder::new(cursor).is_err());
    }

    #[test]
    fn cues_on_frame_returns_only_existing_assets() {
        let mut scene = Scene::new("Szene", 100.0, 100.0);
        let mut sound_layer = Layer::new("Ton", 3);
        sound_layer.kind = LayerKind::Sound;
        let asset = Asset::audio("Ping", "ping.wav", "audio/wav", 8000, 1, 0.01, tiny_wav());
        let missing = Uuid::new_v4();
        sound_layer.frames[0].sound = Some(SoundCue::event(asset.id));
        sound_layer.frames[1].sound = Some(SoundCue {
            asset: missing,
            offset: 0.0,
            duration: 0.0,
            event: true,
            loop_count: 1,
        });
        scene.layers.push(sound_layer);
        let assets = vec![asset];

        let cues = cues_on_frame(&scene, 0, &assets);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].0.name, "Ping");
        // Frame 1's cue references a missing asset: filtered out.
        assert!(cues_on_frame(&scene, 1, &assets).is_empty());
        assert!(cues_on_frame(&scene, 2, &assets).is_empty());
    }

    #[test]
    fn engine_without_device_reports_error_not_panic() {
        // On machines without an audio device `new` stays inert; play()
        // must return an error instead of panicking.
        let mut engine = AudioEngine::new();
        let asset = Asset::audio("Ping", "ping.wav", "audio/wav", 8000, 1, 0.01, tiny_wav());
        if engine.last_error.is_some() {
            assert!(engine.play(&asset).is_err());
        }
    }
}
