// we want to store 30 seconds of audio, 16-bit stereo PCM at 48kHz
// divided into 20ms chunks

pub const DISCORD_AUDIO_CHANNELS: usize = 2;

pub const DISCORD_SAMPLES_PER_SECOND: usize = 48000;
pub const DISCORD_SAMPLES_PER_MILLISECOND: usize = DISCORD_SAMPLES_PER_SECOND / 1000;
pub const DISCORD_PERIOD_PER_PACKET_GROUP_MS: usize = 20;
pub const DISCORD_AUDIO_SAMPLES_PER_PACKET_GROUP_SINGLE_CHANNEL: usize =
    DISCORD_SAMPLES_PER_MILLISECOND * DISCORD_PERIOD_PER_PACKET_GROUP_MS;

// the expected size of a single discord audio update, in samples
pub const DISCORD_PACKET_GROUP_SIZE: usize =
    DISCORD_AUDIO_SAMPLES_PER_PACKET_GROUP_SINGLE_CHANNEL * DISCORD_AUDIO_CHANNELS;

pub const AUDIO_TO_RECORD_SECONDS: usize = 30;
pub const AUDIO_TO_RECORD_MILLISECONDS: usize = AUDIO_TO_RECORD_SECONDS * 1000;

pub const WHISPER_SAMPLES_PER_SECOND: usize = 16000;
pub const WHISPER_SAMPLES_PER_MILLISECOND: usize = WHISPER_SAMPLES_PER_SECOND / 1000;

// the total size of the buffer we'll use to store audio, in samples
pub const WHISPER_AUDIO_BUFFER_SIZE: usize = WHISPER_SAMPLES_PER_SECOND * AUDIO_TO_RECORD_SECONDS;

pub type DiscordAudioSample = i16;
pub type WhisperAudioSample = f32;

pub type Ssrc = u32;

pub(crate) struct DiscordVoiceData {
    pub audio: Vec<DiscordAudioSample>,
    pub timestamp: u32,
    pub ssrc: u32,
}
