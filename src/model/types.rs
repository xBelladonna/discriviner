// we want to store 30 seconds of audio, 16-bit stereo PCM at 48kHz
// divided into 20ms chunks

use std::num::Wrapping;

pub const DISCORD_AUDIO_CHANNELS: usize = 2;

pub const DISCORD_SAMPLES_PER_SECOND: usize = 48000;
// pub const DISCORD_SAMPLES_PER_MILLISECOND: usize = DISCORD_SAMPLES_PER_SECOND / 1000;
// pub const DISCORD_PERIOD_PER_PACKET_GROUP_MS: usize = 20;
// pub const DISCORD_AUDIO_SAMPLES_PER_PACKET_GROUP_SINGLE_CHANNEL: usize =
//     DISCORD_SAMPLES_PER_MILLISECOND * DISCORD_PERIOD_PER_PACKET_GROUP_MS;

// the expected size of a single discord audio update, in samples
// pub const DISCORD_PACKET_GROUP_SIZE: usize =
//     DISCORD_AUDIO_SAMPLES_PER_PACKET_GROUP_SINGLE_CHANNEL * DISCORD_AUDIO_CHANNELS;

pub const AUDIO_TO_RECORD_SECONDS: usize = 30;

pub const WHISPER_SAMPLES_PER_SECOND: usize = 16000;
pub const WHISPER_SAMPLES_PER_MILLISECOND: usize = WHISPER_SAMPLES_PER_SECOND / 1000;

pub const RTC_CLOCK_SAMPLES_PER_SECOND: usize = 8000;

// being a whole number. If this is not the case, we'll need to
// do some more complicated resampling.
pub const BITRATE_CONVERSION_RATIO: usize = DISCORD_SAMPLES_PER_SECOND / WHISPER_SAMPLES_PER_SECOND;

// the total size of the buffer we'll use to store audio, in samples
pub const WHISPER_AUDIO_BUFFER_SIZE: usize = WHISPER_SAMPLES_PER_SECOND * AUDIO_TO_RECORD_SECONDS;

/// If an audio clip is less than this length, we'll ignore it.
pub const MIN_AUDIO_THRESHOLD_MS: u32 = 500;

pub const AUTO_TRANSCRIPTION_PERIOD_MS: usize = 5000;

pub const AUTO_TRANSCRIPTION_PERIOD_SAMPLES: usize =
    WHISPER_SAMPLES_PER_MILLISECOND * AUTO_TRANSCRIPTION_PERIOD_MS;

/// Number of audio buffers to allocate.  This should equal
/// the number of speaking participants we expect to have
/// in a room at a time.
pub const EXPECTED_AUDIO_PARTICIPANTS: usize = 12;

/// keep this many tokens from previous transcriptions, and
/// use them to seed the next transcription.  This is per-user.
pub const TOKENS_TO_KEEP: usize = 1024;

pub const USER_SILENCE_TIMEOUT_MS: u64 = 2000;

pub const DISCORD_AUDIO_MAX_VALUE_TWO_SAMPLES: WhisperAudioSample =
    DISCORD_AUDIO_MAX_VALUE * DISCORD_AUDIO_CHANNELS as WhisperAudioSample;

pub type DiscordAudioSample = i16;
pub type DiscordRtcTimestampInner = u32;
pub type DiscordRtcTimestamp = Wrapping<DiscordRtcTimestampInner>;
pub type Ssrc = u32;
pub type UserId = u64;
pub type WhisperAudioSample = f32;
pub type WhisperToken = i32;

// this is a percentage, so it's between 0 and 100
// use this instead of a float to allow our API types to
// implement Eq, and thus be used as keys in a HashMap
pub type WhisperTokenProbabilityPercentage = u32;

pub const DISCORD_AUDIO_MAX_VALUE: WhisperAudioSample =
    DiscordAudioSample::MAX as WhisperAudioSample;
