// we want to store 30 seconds of audio, 16-bit stereo PCM at 48kHz
// divided into 20ms chunks

use std::time::Duration;

pub(crate) const AUDIO_TO_RECORD_SECONDS: usize = 30;
pub(crate) const AUDIO_TO_RECORD: Duration = Duration::from_secs(AUDIO_TO_RECORD_SECONDS as u64);

/// keep this many tokens from previous transcriptions, and
/// use them to seed the next transcription.  This is per-user.
pub(crate) const TOKENS_TO_KEEP: usize = 1024;

pub(crate) const USER_SILENCE_TIMEOUT_MS: usize = 1000;
pub(crate) const USER_SILENCE_TIMEOUT: Duration =
    Duration::from_millis(USER_SILENCE_TIMEOUT_MS as u64);

pub(crate) const DISCARD_USER_AUDIO_AFTER: Duration = Duration::from_secs(10 * 60);

pub(crate) const WHISPER_SAMPLES_PER_SECOND: usize = 16000;
pub(crate) const WHISPER_SAMPLES_PER_MILLISECOND: usize = 16;

pub(crate) const DISCORD_AUDIO_CHANNELS: usize = 2;
pub(crate) const DISCORD_SAMPLES_PER_SECOND: usize = 48000;

// The RTC timestamp uses an 48khz clock.
pub(crate) const RTC_CLOCK_SAMPLES_PER_MILLISECOND: u128 = 48;

// being a whole number. If this is not the case, we'll need to
// do some more complicated resampling.
pub(crate) const BITRATE_CONVERSION_RATIO: usize =
    DISCORD_SAMPLES_PER_SECOND / WHISPER_SAMPLES_PER_SECOND;

// the total size of the buffer we'll use to store audio, in samples
pub(crate) const WHISPER_AUDIO_BUFFER_SIZE: usize =
    WHISPER_SAMPLES_PER_SECOND * AUDIO_TO_RECORD_SECONDS;

// 31.68 years is BASICALLY forever, said my niece
pub(crate) const FOREVER: Duration = Duration::from_secs(1000 * 1000 * 1000);

pub(crate) const DONT_EVEN_BOTHER_RMS_THRESHOLD: f32 = 0.01;

pub(crate) const ESPEAK_SAMPLES_PER_SECOND: usize = 22050;
