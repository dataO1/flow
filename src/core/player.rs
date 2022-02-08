use crate::core::output;
use log::{info, warn};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use symphonia::core::{formats::Track, io::MediaSourceStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};

// Tokio Channel messages for the player task
#[derive(Clone, Debug)]
pub enum Message {
    Command(Command),
    Response(Response),
}

#[derive(Clone, Debug)]
pub enum Command {
    PlayerStart,
    PlayerStop,
    Unknown,
}

#[derive(Clone, Debug)]
pub enum Response {
    PlayerStarted,
    PlayerStopped,
}

pub struct Player {
    pub reader: Box<dyn FormatReader>,
    pub rx: Receiver<Message>,
    pub tx: Sender<Message>,
}

impl Player {
    //------------------------------------------------------------------//
    //                          Public Methods                          //
    //------------------------------------------------------------------//

    pub fn new(file_path: &str) -> Player {
        let (tx, rx) = channel::<Message>(100);
        let reader = new_reader(file_path);
        Player { reader, rx, tx }
    }

    pub fn toggle_play(reader: &mut Box<dyn FormatReader>) {
        // Store the track identifier, it will be used to filter packets.
        // let track_id = track.id;
        let _res = Player::play_file(reader, None);
    }

    //------------------------------------------------------------------//
    //                         Private methods                          //
    //------------------------------------------------------------------//
    fn play_file(reader: &mut Box<dyn FormatReader>, seek_time: Option<f64>) -> Result<()> {
        // Use the default options for the decoder.
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: true,
            ..Default::default()
        };
        // select the first track with a known codec.
        //
        let track = first_supported_track(reader.tracks());

        let mut track_id = match track {
            Some(track) => track.id,
            _ => return Ok(()),
        };

        // If there is a seek time, seek the reader to the time specified and get the timestamp of the
        // seeked position. All packets with a timestamp < the seeked position will not be played.
        //
        // Note: This is a half-baked approach to seeking! After seeking the reader, packets should be
        // decoded and *samples* discarded up-to the exact *sample* indicated by required_ts. The
        // current approach will discard excess samples if seeking to a sample within a packet.
        let seek_ts = if let Some(time) = seek_time {
            let seek_to = SeekTo::Time {
                time: Time::from(time),
                track_id: Some(track_id),
            };

            // Attempt the seek. If the seek fails, ignore the error and return a seek timestamp of 0 so
            // that no samples are trimmed.
            match reader.seek(SeekMode::Accurate, seek_to) {
                Ok(seeked_to) => seeked_to.required_ts,
                Err(Error::ResetRequired) => {
                    // print_tracks(reader.tracks());
                    track_id = first_supported_track(reader.tracks()).unwrap().id;
                    0
                }
                Err(err) => {
                    // Don't give-up on a seek error.
                    warn!("seek error: {}", err);
                    0
                }
            }
        } else {
            // If not seeking, the seek timestamp is 0.
            0
        };

        // The audio output device.
        let mut audio_output = None;

        let mut track_info = PlayTrackOptions { track_id, seek_ts };

        let result = loop {
            match Player::play_track(reader, &mut audio_output, track_info, &dec_opts) {
                Err(Error::ResetRequired) => {
                    // The demuxer indicated that a reset is required. This is sometimes seen with
                    // streaming OGG (e.g., Icecast) wherein the entire contents of the container change
                    // (new tracks, codecs, metadata, etc.). Therefore, we must select a new track and
                    // recreate the decoder.
                    // print_tracks(self.reader.tracks());

                    // Select the first supported track since the user's selected track number might no
                    // longer be valid or make sense.
                    let track_id = first_supported_track(reader.tracks()).unwrap().id;
                    track_info = PlayTrackOptions {
                        track_id,
                        seek_ts: 0,
                    };
                }
                res => break res,
            }
        };

        // Flush the audio output to finish playing back any leftover samples.
        if let Some(audio_output) = audio_output.as_mut() {
            audio_output.flush()
        }

        result
    }

    fn play_track(
        reader: &mut Box<dyn FormatReader>,
        audio_output: &mut Option<Box<dyn crate::core::output::AudioOutput>>,
        play_opts: PlayTrackOptions,
        decode_opts: &DecoderOptions,
    ) -> Result<()> {
        // Get the selected track using the track ID.
        let track = match reader
            .tracks()
            .iter()
            .find(|track| track.id == play_opts.track_id)
        {
            Some(track) => track,
            _ => return Ok(()),
        };

        // Create a decoder for the track.
        let mut decoder =
            symphonia::default::get_codecs().make(&track.codec_params, decode_opts)?;

        // Get the selected track's timebase and duration.
        let _tb = track.codec_params.time_base;
        let dur = track
            .codec_params
            .n_frames
            .map(|frames| track.codec_params.start_ts + frames);

        // Decode and play the packets belonging to the selected track.
        let result = loop {
            // Get the next packet from the format reader.
            let packet = match reader.next_packet() {
                Ok(packet) => packet,
                Err(err) => break Err(err),
            };

            // If the packet does not belong to the selected track, skip it.
            if packet.track_id() != play_opts.track_id {
                continue;
            }

            //Print out new metadata.
            while !reader.metadata().is_latest() {
                reader.metadata().pop();

                if let Some(rev) = reader.metadata().current() {
                    // print_update(rev);
                }
            }

            // Decode the packet into audio samples.
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    // If the audio output is not open, try to open it.
                    if audio_output.is_none() {
                        // Get the audio buffer specification. This is a description of the decoded
                        // audio buffer's sample format and sample rate.
                        let spec = *decoded.spec();

                        // Get the capacity of the decoded buffer. Note that this is capacity, not
                        // length! The capacity of the decoded buffer is constant for the life of the
                        // decoder, but the length is not.
                        let duration = decoded.capacity() as u64;

                        // Try to open the audio output.
                        audio_output.replace(output::try_open(spec, duration).unwrap());
                    } else {
                        // TODO: Check the audio spec. and duration hasn't changed.
                    }

                    // Write the decoded audio samples to the audio output if the presentation timestamp
                    // for the packet is >= the seeked position (0 if not seeking).
                    if packet.ts() >= play_opts.seek_ts {
                        // print_progress(packet.ts(), dur, tb); //TODO: print progress

                        if let Some(audio_output) = audio_output {
                            audio_output.write(decoded).unwrap()
                        }
                    }
                }
                Err(Error::DecodeError(err)) => {
                    // Decode errors are not fatal. Print the error message and try to decode the next
                    // packet as usual.
                    warn!("decode error: {}", err);
                }
                Err(err) => break Err(err),
            }
        };

        // Regardless of result, finalize the decoder to get the verification result.
        let finalize_result = decoder.finalize();

        if let Some(verify_ok) = finalize_result.verify_ok {
            if verify_ok {
                info!("verification passed");
            } else {
                info!("verification failed");
            }
        }

        result
    }
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
fn new_reader(file_path: &str) -> Box<dyn FormatReader> {
    let src = std::fs::File::open(file_path).expect("failed to open media");

    // Create the media source stream.
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    // Create a probe hint using the file's extension. [Optional]
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    // Use the default options for metadata and format readers.
    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    // Probe the media source.
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .expect("unsupported format");
    // Get the instantiated format reader.
    probed.format
}
