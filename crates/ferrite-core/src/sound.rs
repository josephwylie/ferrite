//! Sounds: the two chimes Ferrite plays when a Thread finished or needs the
//! operator, and playing them (ADR 0017).
//!
//! The built-in chimes are synthesised here as WAV: no audio files in the
//! tree and no audio crate. Playback hands a file to the platform's own
//! player on a thread of its own — `afplay` on macOS, PowerShell's
//! `SoundPlayer` on Windows (which plays WAV only). A sound that cannot play
//! is skipped; the operator's own file that cannot play falls back to the
//! built-in chime.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::spawn::NoConsoleWindow;

/// Which chime: a Thread finished, or a Thread needs the operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Done,
    NeedsYou,
}

/// The chimes' sample rate: plenty for two short bell tones.
const RATE: u32 = 22_050;

impl Sound {
    /// The chime's notes: each one's pitch (Hz) and length (ms). Done rises
    /// a fourth and settles; needs-you rings twice on a higher note.
    fn notes(self) -> &'static [(f32, u32)] {
        match self {
            Sound::Done => &[(659.25, 110), (880.0, 320)],
            Sound::NeedsYou => &[(987.77, 130), (987.77, 260)],
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Sound::Done => "done",
            Sound::NeedsYou => "needs-you",
        }
    }

    /// The built-in chime as a mono 16-bit PCM WAV file: each note a sine
    /// with a soft octave over it, a 5 ms attack, an exponential decay and a
    /// 20 ms release, so it reads as a bell and never clicks.
    pub fn wav(self) -> Vec<u8> {
        let mut samples: Vec<i16> = Vec::new();
        for &(pitch, ms) in self.notes() {
            let count = RATE * ms / 1000;
            let attack = (RATE / 200) as f32;
            let release = (RATE / 50) as f32;
            for at in 0..count {
                let t = at as f32 / RATE as f32;
                let envelope = (at as f32 / attack).min(1.0)
                    * ((count - at) as f32 / release).min(1.0)
                    * (-t * 9.0).exp();
                let phase = std::f32::consts::TAU * pitch * t;
                let tone = phase.sin() + 0.25 * (2.0 * phase).sin();
                samples.push((tone * envelope * 0.3 * f32::from(i16::MAX)) as i16);
            }
        }
        let data = (samples.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
        wav.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        wav
    }
}

/// Play `sound` without waiting for it: the operator's `file` when given
/// and playable, else the built-in chime.
pub fn play(sound: Sound, file: Option<&Path>) {
    let file = file.map(Path::to_path_buf);
    std::thread::spawn(move || play_with(sound, file.as_deref(), run_player));
}

/// `play`'s work, through `run` (which plays a file to its end and says
/// whether it could).
fn play_with(sound: Sound, file: Option<&Path>, run: impl Fn(&Path) -> bool) {
    if file.is_some_and(&run) {
        return;
    }
    // The players read files, so the chime is written out for each play
    // and removed once it has played.
    static PLAYS: AtomicU64 = AtomicU64::new(0);
    let path: PathBuf = std::env::temp_dir().join(format!(
        "ferrite-{}-{}-{}.wav",
        sound.slug(),
        std::process::id(),
        PLAYS.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&path, sound.wav()).is_ok() {
        run(&path);
    }
    let _ = std::fs::remove_file(&path);
}

/// Play the file at `path` to its end. False when it could not be played.
fn run_player(path: &Path) -> bool {
    let Some(mut player) = player(path) else {
        return false;
    };
    player
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .no_console_window()
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "macos")]
fn player(path: &Path) -> Option<Command> {
    let mut afplay = Command::new("afplay");
    afplay.arg(path);
    Some(afplay)
}

/// The path rides an environment variable, so no quoting can break the
/// script.
#[cfg(windows)]
fn player(path: &Path) -> Option<Command> {
    let mut powershell = Command::new("powershell");
    powershell
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(New-Object Media.SoundPlayer $env:FERRITE_SOUND).PlaySync()",
        ])
        .env("FERRITE_SOUND", path);
    Some(powershell)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn player(_path: &Path) -> Option<Command> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// Each chime is a well-formed mono 16-bit PCM WAV whose length is its
    /// notes', and the two are told apart by ear.
    #[test]
    fn the_chimes_are_wav_files_of_their_notes() {
        for sound in [Sound::Done, Sound::NeedsYou] {
            let wav = sound.wav();
            assert_eq!(&wav[0..4], b"RIFF");
            assert_eq!(&wav[8..16], b"WAVEfmt ");
            assert_eq!(le_u32(&wav, 4) as usize, wav.len() - 8);
            assert_eq!(le_u32(&wav, 24), RATE);
            assert_eq!(&wav[36..40], b"data");
            let ms: u32 = sound.notes().iter().map(|(_, ms)| ms).sum();
            let frames = le_u32(&wav, 40) / 2;
            assert!(frames.abs_diff(RATE * ms / 1000) <= 2, "{sound:?}");
            // Short enough never to talk over the next one.
            assert!(ms < 500, "{sound:?}");
        }
        assert_ne!(Sound::Done.wav(), Sound::NeedsYou.wav());
    }

    /// The operator's file plays alone when it can; one that cannot falls
    /// back to the built-in chime, written out for the player and then
    /// removed.
    #[test]
    fn an_unplayable_file_falls_back_to_the_built_in_chime() {
        use std::cell::RefCell;
        let own = Path::new("/sounds/bell.aiff");
        let played = RefCell::new(Vec::new());
        play_with(Sound::Done, Some(own), |path| {
            played.borrow_mut().push(path.to_path_buf());
            true
        });
        assert_eq!(*played.borrow(), [own]);

        let played = RefCell::new(Vec::new());
        play_with(Sound::NeedsYou, Some(own), |path| {
            let wav = (path != own).then(|| std::fs::read(path).unwrap());
            played.borrow_mut().push((path.to_path_buf(), wav));
            path != own
        });
        let played = played.into_inner();
        assert_eq!(played.len(), 2);
        assert_eq!(played[0], (own.to_path_buf(), None));
        let (chime, wav) = &played[1];
        assert_eq!(wav.as_deref(), Some(&Sound::NeedsYou.wav()[..]));
        assert!(!chime.exists(), "the written chime is removed");
    }

    /// Every note fades in and out: no sample at either side of a note's
    /// edge is loud enough to click.
    #[test]
    fn every_note_starts_and_ends_quietly() {
        let wav = Sound::NeedsYou.wav();
        let samples: Vec<i16> = wav[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let first = (RATE * Sound::NeedsYou.notes()[0].1 / 1000) as usize;
        for edge in [0, first - 1, first, samples.len() - 1] {
            assert!(samples[edge].unsigned_abs() < 200, "sample {edge}");
        }
    }
}
