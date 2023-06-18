use std::{
    cmp::max,
    num::Wrapping,
    time::{Duration, SystemTime},
};

use bytes::Bytes;

use super::{
    constants::{
        AUDIO_TO_RECORD, AUDIO_TO_RECORD_SECONDS, AUTO_TRANSCRIPTION_PERIOD_MS,
        USER_SILENCE_TIMEOUT,
    },
    types::{
        self, DiscordAudioSample, DiscordRtcTimestamp, DiscordRtcTimestampInner, Transcription,
        WhisperAudioSample,
    },
};

const DISCORD_AUDIO_CHANNELS: usize = 2;
const DISCORD_SAMPLES_PER_SECOND: usize = 48000;

const WHISPER_SAMPLES_PER_SECOND: usize = 16000;
const WHISPER_SAMPLES_PER_MILLISECOND: usize = 16;

// The RTC timestamp uses an 48khz clock.
const RTC_CLOCK_SAMPLES_PER_MILLISECOND: u128 = 48;

// being a whole number. If this is not the case, we'll need to
// do some more complicated resampling.
const BITRATE_CONVERSION_RATIO: usize = DISCORD_SAMPLES_PER_SECOND / WHISPER_SAMPLES_PER_SECOND;

// the total size of the buffer we'll use to store audio, in samples
const WHISPER_AUDIO_BUFFER_SIZE: usize = WHISPER_SAMPLES_PER_SECOND * AUDIO_TO_RECORD_SECONDS;

const DISCORD_AUDIO_MAX_VALUE: WhisperAudioSample = DiscordAudioSample::MAX as WhisperAudioSample;

pub(crate) const DISCORD_AUDIO_MAX_VALUE_TWO_SAMPLES: WhisperAudioSample =
    DISCORD_AUDIO_MAX_VALUE * DISCORD_AUDIO_CHANNELS as WhisperAudioSample;

fn duration_to_rtc(duration: &Duration) -> DiscordRtcTimestamp {
    let rtc_samples = duration.as_millis() * RTC_CLOCK_SAMPLES_PER_MILLISECOND;
    Wrapping(rtc_samples as DiscordRtcTimestampInner)
}

fn rtc_timestamp_to_index(ts1: DiscordRtcTimestamp, ts2: DiscordRtcTimestamp) -> usize {
    let delta = (ts2 - ts1).0 as usize;
    // we want the number of 16khz samples, so just multiply by 2.
    delta * WHISPER_SAMPLES_PER_MILLISECOND / RTC_CLOCK_SAMPLES_PER_MILLISECOND as usize
}

fn discord_samples_to_whisper_samples(samples: usize) -> usize {
    samples / (BITRATE_CONVERSION_RATIO * DISCORD_AUDIO_CHANNELS)
}

fn samples_to_duration(num_samples: usize) -> u64 {
    (num_samples / WHISPER_SAMPLES_PER_MILLISECOND) as u64
}

pub(crate) struct LastRequestInfo {
    pub start_time: SystemTime,
    pub original_duration: Duration,
    pub audio_trimmed_since_request: Duration,
    pub in_progress: bool,
    pub requested_at: SystemTime,
    pub final_request: bool,
}

impl LastRequestInfo {
    pub fn effective_duration(&self) -> Duration {
        self.original_duration - self.audio_trimmed_since_request
    }
}

pub(crate) struct AudioSlice {
    pub audio: Vec<WhisperAudioSample>,
    pub finalized: bool,
    pub last_request: Option<LastRequestInfo>,
    pub slice_id: u64,
    pub start_time: Option<(DiscordRtcTimestamp, SystemTime)>,
    pub tentative_transcript_opt: Option<Transcription>,
}

impl AudioSlice {
    pub fn new(slice_id: u64) -> Self {
        Self {
            audio: Vec::with_capacity(WHISPER_AUDIO_BUFFER_SIZE),
            finalized: false,
            last_request: None,
            slice_id,
            start_time: None,
            tentative_transcript_opt: None,
        }
    }

    pub fn clear(&mut self) {
        eprintln!("{}: clearing audio slice", self.slice_id);
        self.audio.clear();
        self.finalized = false;
        self.last_request = None;
        self.start_time = None;
        self.tentative_transcript_opt = None;
    }

    /// True if the given timestamp is within the bounds of this slice.
    /// Bounds are considered to begin with the start of the slice,
    /// and end within the given timeout of the end of the slice.
    /// An empty slice is considered to have no bounds, and will fit
    /// any timestamp.
    pub fn fits_within_this_slice(&self, rtc_timestamp: DiscordRtcTimestamp) -> bool {
        if let Some((start_rtc, _)) = self.start_time {
            let current_end = start_rtc + duration_to_rtc(&self.buffer_duration());

            // add end of buffer
            // note: this will ignore the size of the audio we're looking to
            // add, but that's ok
            let timeout = duration_to_rtc(&AUDIO_TO_RECORD);
            let end = current_end + timeout;

            let result;
            if start_rtc < end {
                result = rtc_timestamp >= start_rtc && rtc_timestamp < end
            } else {
                // if the slice wraps around, then we need to check
                // if the timestamp is either before the end or after
                // the start.
                result = rtc_timestamp < end || rtc_timestamp >= start_rtc
            }

            if !result {
                eprintln!(
                    "{}: timestamp {} does not fit within slice.  start={:?} end={:?}",
                    self.slice_id, rtc_timestamp, self.start_time, end,
                )
            }
            result
        } else {
            // this is a blank slice, so any timestamp fits
            true
        }
    }

    pub fn add_audio(
        &mut self,
        rtc_timestamp: DiscordRtcTimestamp,
        discord_audio: &[DiscordAudioSample],
    ) {
        if !self.fits_within_this_slice(rtc_timestamp) {
            // if the timestamp is not within the bounds of this slice,
            // then we need to create a new slice.
            eprintln!(
                "{}: trying to add audio to inactive slice, dropping audio",
                self.slice_id
            );
            return;
        }
        self.finalized = false;

        let start_index;
        if let Some((start_rtc, _)) = self.start_time {
            start_index = rtc_timestamp_to_index(start_rtc, rtc_timestamp);
        } else {
            // this is the first audio for the slice, so we need to set
            // the start time
            self.start_time = Some((rtc_timestamp, SystemTime::now()));
            start_index = 0;
        }

        self.resample_audio_from_discord_to_whisper(start_index, discord_audio);

        // if self.tentative_transcript_opt.is_some() {
        //     eprintln!("discarding tentative transcription");
        // }

        // update the last slice to point to the end of the buffer
    }

    /// Transcode the audio into the given location of the buffer,
    /// converting it from Discord's format (48khz stereo PCM16)
    /// to Whisper's format (16khz mono f32).
    ///
    /// This handles several cases:
    ///  - a single allocation for the new audio at the end of the buffer
    ///  - also, inserting silence if the new audio is not contiguous with
    ///    the previous audio
    ///  - doing it in a way that we can also backfill audio if we get
    ///    packets out-of-order
    ///
    fn resample_audio_from_discord_to_whisper(
        &mut self,
        start_index: usize,
        discord_audio: &[DiscordAudioSample],
    ) {
        let end_index = start_index + discord_samples_to_whisper_samples(discord_audio.len());
        let buffer_len = max(self.audio.len(), end_index);

        self.audio.resize(buffer_len, WhisperAudioSample::default());

        let dest_buf = &mut self.audio[start_index..end_index];

        for (i, samples) in discord_audio
            .chunks_exact(BITRATE_CONVERSION_RATIO * DISCORD_AUDIO_CHANNELS)
            .enumerate()
        {
            // sum the channel data, and divide by the max value possible to
            // get a value between -1.0 and 1.0
            dest_buf[i] = samples
                .iter()
                .take(DISCORD_AUDIO_CHANNELS)
                .map(|x| *x as types::WhisperAudioSample)
                .sum::<types::WhisperAudioSample>()
                / DISCORD_AUDIO_MAX_VALUE_TWO_SAMPLES;
        }
    }

    fn is_ready_for_transcription(&self, user_silent: bool) -> bool {
        if self.start_time.is_none() {
            return false;
        }

        if self.finalized {
            // the slice is finalized, but we haven't requested the full
            // buffer yet, so request it
            return true;
        }

        if let Some(last_request) = self.last_request.as_ref() {
            if last_request.in_progress {
                // we already have a pending request, so we don't need to
                // make another one
                return false;
            }
        }

        if user_silent {
            // if the user is silent, then we need to request the full
            // buffer, even if no period shift has occurred
            // in this case we'll have two outstanding transcription
            // requests...
            return true;
        }

        let current_period = self.buffer_duration().as_millis() / AUTO_TRANSCRIPTION_PERIOD_MS;
        let last_period;
        if let Some(last_request_info) = self.last_request.as_ref() {
            last_period =
                last_request_info.effective_duration().as_millis() / AUTO_TRANSCRIPTION_PERIOD_MS;
        } else {
            last_period = 0;
        }

        last_period != current_period
    }

    pub fn make_transcription_request(
        &mut self,
        user_idle: bool,
    ) -> Option<(Bytes, Duration, SystemTime)> {
        if !self.is_ready_for_transcription(user_idle) {
            return None;
        }
        if let Some((_, start_time)) = self.start_time {
            let buffer = self.audio.as_slice();
            let buffer_len_bytes = std::mem::size_of_val(buffer);
            let byte_data = unsafe {
                std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer_len_bytes)
            };

            let duration = self.buffer_duration();
            eprintln!(
                "{}: requesting transcription for {} ms",
                self.slice_id,
                duration.as_millis()
            );
            let new_request = LastRequestInfo {
                start_time,
                audio_trimmed_since_request: Duration::ZERO,
                original_duration: self.buffer_duration(),
                in_progress: true,
                requested_at: SystemTime::now(),
                final_request: self.finalized,
            };
            if let Some(last_request) = self.last_request.as_ref() {
                if last_request.start_time == new_request.start_time {
                    if last_request.original_duration == new_request.original_duration {
                        eprintln!("{}: discarding duplicate request", self.slice_id);
                        // if this is our final request, make sure the last request
                        // has the final flag set
                        if new_request.final_request {
                            self.last_request.as_mut().unwrap().final_request = true;
                        }
                        return None;
                    }
                }
                if last_request.in_progress {
                    eprintln!("{}: discarding previous in-progress request", self.slice_id);
                }
            }
            self.last_request = Some(new_request);
            return Some((Bytes::from(byte_data), duration, start_time));
        }
        None
    }

    /// Discards the amount of audio specified by the duration
    /// from the start of the buffer, shuffling the remaining
    /// audio to the start of the buffer.  Any indexes and
    /// timestamps are adjusted accordingly.
    pub fn discard_audio(&mut self, duration: &Duration) {
        let discard_idx = duration.as_millis() as usize * WHISPER_SAMPLES_PER_MILLISECOND;

        if duration.is_zero() {
            return;
        }

        eprintln!(
            "discarding {} ms of audio from {} ms buffer",
            duration.as_millis(),
            self.buffer_duration().as_millis()
        );

        if discard_idx >= self.audio.len() {
            // discard as much as we have, so just clear the buffer
            self.clear();
            return;
        }

        // eliminate this many samples from the start of the buffer
        self.audio.drain(0..discard_idx);

        // update the start timestamp
        if let Some((start_rtc, start_system)) = self.start_time {
            self.start_time = Some((
                start_rtc + duration_to_rtc(duration),
                start_system + *duration,
            ));
        }

        // also update the last_request duration
        // call trim_audio to update the last_request
        if let Some(last_request) = self.last_request.as_mut() {
            last_request.audio_trimmed_since_request += *duration;
        }
    }

    pub fn finalize(&mut self) -> Option<Transcription> {
        self.finalized = true;

        eprintln!(
            "finalizing slice with {} ms of audio",
            self.buffer_duration().as_millis()
        );

        if self.tentative_transcript_opt.is_none() {
            // if we don't have a tentative transcription, then
            // we can't return anything
            // eprintln!("no tentative transcription in finalize, returning None");
            return None;
        }

        let tentative_transcript = self.tentative_transcript_opt.take().unwrap();

        if tentative_transcript.segments.len() > 0 {
            eprintln!(
                "tentative description has {} segments, covering {} ms",
                tentative_transcript.segments.len(),
                tentative_transcript.audio_duration.as_millis(),
            );
        }

        // if we had a tentative transcription, return it.
        // We know that it's current, since if we had gathered
        // more audio, we would have discarded it.
        if tentative_transcript.audio_duration != self.buffer_duration() {
            eprintln!(
                "tentative transcription duration {} != buffer duration {}",
                tentative_transcript.audio_duration.as_millis(),
                self.buffer_duration().as_millis()
            );
            return None;
        }
        self.clear();

        Some(tentative_transcript)
    }

    pub fn buffer_duration(&self) -> Duration {
        Duration::from_millis(samples_to_duration(self.audio.len()))
    }

    pub fn handle_transcription_response(
        &mut self,
        message: &Transcription,
    ) -> Option<Transcription> {
        // if we had more than one request in flight, we need to
        // ignore all but the latest
        if let Some(last_request) = self.last_request.as_ref() {
            if last_request.start_time != message.start_timestamp {
                eprintln!(
                    "ignoring transcription response with start time {:?} (expected {:?})",
                    message.start_timestamp, last_request.start_time
                );
                return None;
            }
            if message.audio_duration != last_request.original_duration {
                eprintln!(
                    "ignoring transcription response with duration {:?} (expected {:?})",
                    message.audio_duration, last_request.original_duration
                );
                return None;
            }
        } else {
            // this can happen if we've cleared the buffer but had
            // an outstanding transcription request
            eprintln!("ignoring transcription response with no last request");
            return None;
        }
        self.last_request.as_mut().unwrap().in_progress = false;

        // figure out how many segments have an end time that's more
        // than USER_SILENCE_TIMEOUT ago.  Those will be returned to
        // the caller in a Transcription.
        // The remainder, if any, will be kept in tentative_transcription,
        // but only if we haven't seen new audio since the response was generated.

        let end_time = if self.last_request.as_ref().unwrap().final_request {
            // if this is the final request, then we want to keep all
            // segments
            SystemTime::now() + Duration::from_secs(1000)
        } else {
            self.last_request.as_ref().unwrap().requested_at - USER_SILENCE_TIMEOUT
        };
        let (finalized_transcript, tentative_transcript) =
            Transcription::split_at_end_time(message, end_time);
        // if self.finalized {
        //     assert!(tentative_transcript.is_empty());
        // }
        assert_eq!(
            finalized_transcript.audio_duration + tentative_transcript.audio_duration,
            message.audio_duration
        );
        // todo: check to see if the tentative transcription is accurate

        eprintln!(
            "have transcription: {} final segments ({} ms), {} tentative segments ({} ms)",
            finalized_transcript.segments.len(),
            finalized_transcript.audio_duration.as_millis(),
            tentative_transcript.segments.len(),
            tentative_transcript.audio_duration.as_millis(),
        );

        // eprintln!("finalized transcription: '{}'", finalized_transcript.text(),);
        // eprintln!("tentative transcription: '{}'", tentative_transcript.text(),);

        // with our finalized transcription, we can now discard
        // the audio that was used to generate it.  Be sure to
        // only discard exactly as much audio as was represented
        // by the finalized transcription, or the times will not line up.
        self.discard_audio(&finalized_transcript.audio_duration);

        // if the remaining audio length is the same as the tentative
        // transcription, that means no new audio has arrived in the
        // meantime, so we can keep the tentative transcription.
        eprintln!(
            "tentative transcription with {} segments ({} ms): '{}'",
            tentative_transcript.segments.len(),
            tentative_transcript.audio_duration.as_millis(),
            tentative_transcript.text(),
        );

        if self.buffer_duration() == tentative_transcript.audio_duration {
            // eprintln!("keeping tentative transcription");
            self.tentative_transcript_opt = Some(tentative_transcript);
        } else {
            // eprintln!("discarding tentative transcription");
            self.tentative_transcript_opt = None;
        }

        if finalized_transcript.is_empty() {
            None
        } else {
            Some(finalized_transcript)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_discard_audio() {
        let mut slice = AudioSlice::new(123);
        slice.start_time = Some((
            Wrapping(1000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32),
            SystemTime::now(),
        ));
        slice.audio = vec![0.0; 1000 * WHISPER_SAMPLES_PER_MILLISECOND];
        assert_eq!(slice.buffer_duration(), Duration::from_millis(1000));

        slice.discard_audio(&Duration::from_millis(500));

        assert_eq!(slice.buffer_duration(), Duration::from_millis(500));
        assert_eq!(slice.audio.len(), 500 * WHISPER_SAMPLES_PER_MILLISECOND);
        let time = slice.start_time.unwrap().0;
        assert_eq!(time.0, 500 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32);
    }

    const DISCORD_SAMPLES_PER_MILLISECOND: usize = DISCORD_SAMPLES_PER_SECOND / 1000;
    #[test]
    fn test_add_audio() {
        let mut slice = AudioSlice::new(234);
        slice.start_time = Some((
            Wrapping(1000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32),
            SystemTime::now(),
        ));
        slice.audio = vec![0.0; 1000 * WHISPER_SAMPLES_PER_MILLISECOND];
        assert_eq!(slice.buffer_duration(), Duration::from_millis(1000));

        slice.add_audio(
            Wrapping(2000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32),
            &vec![1; 500 * DISCORD_SAMPLES_PER_MILLISECOND * DISCORD_AUDIO_CHANNELS],
        );

        assert_eq!(slice.buffer_duration(), Duration::from_millis(1500));
        assert_eq!(slice.audio.len(), 1500 * WHISPER_SAMPLES_PER_MILLISECOND);
        let time = slice.start_time.unwrap().0;
        assert_eq!(time.0, 1000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32);

        slice.add_audio(
            Wrapping(4000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32),
            &vec![1; 500 * DISCORD_SAMPLES_PER_MILLISECOND * DISCORD_AUDIO_CHANNELS],
        );

        assert_eq!(slice.buffer_duration(), Duration::from_millis(3500));
        assert_eq!(slice.audio.len(), 3500 * WHISPER_SAMPLES_PER_MILLISECOND);
        let time = slice.start_time.unwrap().0;
        assert_eq!(time.0, 1000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32);

        // don't add audio that's too far in the future
        slice.add_audio(
            Wrapping(8000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32),
            &vec![1; 500 * DISCORD_SAMPLES_PER_MILLISECOND * DISCORD_AUDIO_CHANNELS],
        );

        assert_eq!(slice.buffer_duration(), Duration::from_millis(3500));
        assert_eq!(slice.audio.len(), 3500 * WHISPER_SAMPLES_PER_MILLISECOND);
        let time = slice.start_time.unwrap().0;
        assert_eq!(time.0, 1000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32);

        assert!(
            slice.fits_within_this_slice(Wrapping(1000 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32))
        );

        assert!(
            !slice.fits_within_this_slice(Wrapping(999 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32))
        );

        assert!(
            slice.fits_within_this_slice(Wrapping(6499 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32))
        );

        assert!(!slice
            .fits_within_this_slice(Wrapping(6500 * RTC_CLOCK_SAMPLES_PER_MILLISECOND as u32)));
    }
}
