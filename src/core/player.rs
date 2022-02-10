use crate::core::output;
use log::warn;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
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
    // Load a new file
    Load(String),
    // Toggle playback
    TogglePlay,
    // Stop playback and return to beginning of the track
    Stop,
    // Start playing in "Cue" mode (on CueStop the player resumes to the point of the track, where
    // Cue got invoked)
    Cue,
    // Stop playback and resume to start of Cue
    CueStop,
    // Close the player
    Close,
}

#[derive(Clone, Debug)]
pub enum Response {
    Started,
    Stopped,
}

pub struct Player {
    // pub reader: Box<dyn FormatReader>,
// pub rx: Receiver<Message>,
// pub tx: Sender<Message>,
}

struct PlayerState {
    // loaded file
    loaded: Option<String>,
    // is player playing right now
    playing: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            loaded: None,
            playing: false,
        }
    }
}

impl Player {
    //------------------------------------------------------------------//
    //                          Public Methods                          //
    //------------------------------------------------------------------//

    /// Initializes a new thread, that handles Commands.
    pub fn spawn() -> Sender<Message> {
        // The async channels for Commands(tx) and Responses(rx)
        let (tx, rx) = channel::<Message>(1000);
        // Start the command handler thread
        tokio::spawn(async move { Player::event_loop(rx).await });
        tx
    }

    //------------------------------------------------------------------//
    //                         Command Handlers                         //
    //------------------------------------------------------------------//
    async fn event_loop(mut rx: Receiver<Message>) {
        // The player state
        let mut player_state = PlayerState::default();
        // TODO: move these into PlayerState
        // The reader/decoder
        let mut reader = None;
        // audio output handle
        let mut audio_output = None;
        // decoder
        let mut decoder = None;
        // play options
        let mut play_opts = None;

        // Async event handlers here:
        loop {
            // command handlers
            match rx.try_recv() {
                Ok(Message::Command(Command::Load(path))) => {
                    println!("Received Load Command");
                    let mut r = Player::new_reader(&path);
                    let (dec, po) = Player::init_output(&mut r, Some(42_f64)).unwrap();
                    reader.replace(r);
                    play_opts.replace(po);
                    decoder.replace(dec);

                    player_state.loaded = Some(path);
                }
                Ok(Message::Command(Command::TogglePlay)) => {
                    player_state.playing ^= true;
                    println!(
                        "Received TogglePlay. New playing state is now {}",
                        player_state.playing
                    );
                }
                Ok(Message::Command(Command::Close)) => break,
                Ok(msg) => todo!("{:#?}", msg),
                Err(_) => {
                    // This happens, when there are still outstanding channels, but the message
                    // queue is empty, so just ignore this
                }
            }
            // if there is a valid reader, play_options and decoder play a sample
            if let (Some(r), Some(p_opts), Some(dec)) = (&mut reader, &mut play_opts, &mut decoder)
            {
                if player_state.playing {
                    Player::play_sample(r, &mut audio_output, p_opts, dec).unwrap();
                }
            };
        }
    }
    fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
        tracks
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    }

    // creates a new @FormatReader
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

    fn init_output(
        reader: &mut Box<dyn FormatReader>,
        seek_time: Option<f64>,
    ) -> Result<(Box<dyn Decoder>, PlayTrackOptions)> {
        // Use the default options for the decoder.
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: true,
            ..Default::default()
        };
        // select the first track with a known codec.
        //
        let track = Player::first_supported_track(reader.tracks());
        let codec_params = &track.unwrap().codec_params;
        let decoder = symphonia::default::get_codecs().make(&codec_params, &dec_opts);
        let mut track_id = match track {
            Some(track) => track.id,
            _ => 0,
        };

        // The audio output device.
        // let mut audio_output = None;

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
                    track_id = Player::first_supported_track(reader.tracks()).unwrap().id;
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

        let track_info = PlayTrackOptions { track_id, seek_ts };
        //TODO: do we need this loop for non-streamed formats?
        // Player::play_track(reader, rx, &mut audio_output, track_info, &dec_opts);
        //
        // Create a decoder for the track.
        match decoder {
            Ok(dec) => Ok((dec, track_info)),
            Err(err) => Err(err),
        }
    }

    // // TODO: refactor
    // async fn play_file(
    //     reader: &mut Box<dyn FormatReader>,
    //     rx: &mut Receiver<Message>,
    //     seek_time: Option<f64>,
    // ) -> Result<()> {
    //     let result = loop {
    //         match Player::play_sample(reader, rx, track_info, &dec_opts) {
    //             Err(Error::ResetRequired) => {
    //                 // The demuxer indicated that a reset is required. This is sometimes seen with
    //                 // streaming OGG (e.g., Icecast) wherein the entire contents of the container change
    //                 // (new tracks, codecs, metadata, etc.). Therefore, we must select a new track and
    //                 // recreate the decoder.
    //                 // print_tracks(self.reader.tracks());
    //
    //                 // Select the first supported track since the user's selected track number might no
    //                 // longer be valid or make sense.
    //                 let track_id = Player::first_supported_track(reader.tracks()).unwrap().id;
    //                 track_info = PlayTrackOptions {
    //                     track_id,
    //                     seek_ts: 0,
    //                 };
    //             }
    //             res => break res,
    //         }
    //     };
    //
    //     // Flush the audio output to finish playing back any leftover samples.
    //     if let Some(audio_output) = audio_output.as_mut() {
    //         audio_output.flush()
    //     }
    //
    //     result
    // }

    fn play_sample(
        reader: &mut Box<dyn FormatReader>,
        audio_output: &mut Option<Box<dyn crate::core::output::AudioOutput>>,
        play_opts: &mut PlayTrackOptions,
        decoder: &mut Box<dyn Decoder>,
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
        // Get the selected track's timebase and duration.
        let _tb = track.codec_params.time_base;
        // let dur = track
        //     .codec_params
        //     .n_frames
        //     .map(|frames| track.codec_params.start_ts + frames);

        // Decode and play the packets belonging to the selected track.
        // Get the next packet from the format reader.
        let packet = reader.next_packet().unwrap();

        // If the packet does not belong to the selected track, skip it.
        if packet.track_id() != play_opts.track_id {
            ()
        }

        // //Print out new metadata.
        // while !reader.metadata().is_latest() {
        //     reader.metadata().pop();
        //
        //     // if let Some(rev) = reader.metadata().current() {
        //     //     // print_update(rev);
        //     // }
        // }

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
            // TODO: catch these errors
            Err(Error::IoError(_)) => (),
            Err(Error::SeekError(_)) => (),
            Err(Error::Unsupported(_)) => (),
            Err(Error::LimitError(_)) => (),
            Err(Error::ResetRequired) => (),
        };

        // Regardless of result, finalize the decoder to get the verification result.
        // let finalize_result = decoder.finalize();

        // if let Some(verify_ok) = finalize_result.verify_ok {
        //     if verify_ok {
        //         info!("verification passed");
        //     } else {
        //         info!("verification failed");
        //     }
        // }
        Ok(())
    }
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}
