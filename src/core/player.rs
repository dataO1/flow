use std::sync::{Arc, Mutex};

use crate::core::player;
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::warn;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::audio::{Channels, SignalSpec};
use symphonia::core::codecs::Decoder;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::FormatReader;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

pub enum Message {
    /// Load a new file
    Load(String),
    /// Toggle playback
    TogglePlay,
    /// Stop playback and return to beginning of the track
    Stop,
    /// Start playing in "Cue" mode (on CueStop the player resumes to the point of the track, where
    /// Cue got invoked)
    Cue,
    /// Stop playback and resume to start of Cue
    CueStop,
    /// Close the player
    Close,
    /// Get missing preview Data. The parameter tells the player how many preview samples the app
    /// already has
    GetPreview(usize),
}

pub enum Event {
    // Preview(Box<usizeBuffer>),
// Played x messages
// PlayedPackages(usize),
}

#[derive(Copy, Clone, PartialEq)]
pub enum PlayerState {
    Unloaded,
    Paused,
    Playing,
    Closed,
}

pub struct Player {
    /// player state
    state: PlayerState,
    /// player position in packages
    position: Arc<Mutex<usize>>,
    /// Formatreader
    reader: Option<Box<dyn FormatReader>>,
    /// Decoder
    decoder: Option<Box<dyn Decoder>>,
    /// PulseAudio output
    output: Option<psimple::Simple>,
    /// Signal Spec
    spec: Option<SignalSpec>,
}

impl Player {
    //------------------------------------------------------------------//
    //                          Public Methods                          //
    //------------------------------------------------------------------//

    /// Initializes a new thread, that handles Commands.
    /// Returns a Sender, which can be used to send messages to the player
    pub fn spawn(
        player_position: Arc<Mutex<usize>>,
        player_message_in: Receiver<player::Message>,
        player_event_out: Sender<player::Event>,
    ) -> JoinHandle<()> {
        // The async channel for Events from the reader
        // Start the command handler thread
        tokio::spawn(async move {
            let mut player = Player::new(player_position);
            player.event_loop(player_message_in, player_event_out).await
        })
    }

    fn new(position: Arc<Mutex<usize>>) -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        Self {
            state: PlayerState::Unloaded,
            position,
            reader: None,
            decoder: None,
            output: None,
            spec: None,
        }
    }

    async fn event_loop(
        &mut self,
        mut player_message_in: Receiver<Message>,
        player_event_out: Sender<player::Event>,
    ) {
        while self.state != PlayerState::Closed {
            // command handlers
            match player_message_in.try_recv() {
                //------------------------------------------------------------------//
                //                           App Messages                           //
                //------------------------------------------------------------------//
                Ok(Message::Load(path)) => {
                    // Communicate to the reader, that we want to load a track
                    self.load(path);
                }
                Ok(Message::TogglePlay) => {
                    self.toggle_play();
                }
                Ok(Message::Close) => break,
                Ok(_msg) => {
                    todo!()
                }
                Err(_) => {
                    // This happens, when there are still outstanding channels, but the message
                    // queue is empty, so just ignore this
                }
            }
            // play buffered packets
            if let PlayerState::Playing = self.state {
                if let Some(_) = &mut self.output {
                    self.play();
                }
            }
        }
    }
    fn load(&mut self, path: String) {
        self.init_reader(path);
        self.init_decoder();
        self.init_output();
        self.state = PlayerState::Paused;
        *self.position.lock().unwrap() = 0;
    }

    fn pause(&mut self) {
        if let Some(out) = &mut self.output {
            out.flush();
        }
    }

    fn toggle_play(&mut self) {
        // check if audio output is valid
        if let Some(_) = &mut self.output {
            match self.state {
                PlayerState::Paused => self.state = PlayerState::Playing,
                PlayerState::Playing => {
                    self.state = PlayerState::Paused;
                    self.pause();
                }
                PlayerState::Unloaded => {
                    // do nothing, player not ready yet
                }
                PlayerState::Closed => {
                    // this should be impossibles!
                }
            }
        };
    }

    fn play(&mut self) {
        *self.position.lock().unwrap() += 1;
        match (&mut self.reader, &mut self.decoder, &mut self.output) {
            (Some(reader), Some(decoder), Some(out)) => {
                let packet = reader.next_packet().unwrap();
                let decoded = decoder.decode(&packet).unwrap();
                let mut raw_sample_buf =
                    RawSampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                raw_sample_buf.copy_interleaved_ref(decoded);
                match out.write(raw_sample_buf.as_bytes()) {
                    Ok(_) => {
                        // successfully wrote buffer
                        // println!("success");
                    }
                    Err(err) => {
                        // PAErr
                        // println!("Error: {}", err);
                    }
                }
            }
            _ => {
                println!("Not everything was initialized")
            }
        }
    }

    /// Maps a set of Symphonia `Channels` to a PulseAudio channel map.
    fn map_channels_to_pa_channelmap(channels: Channels) -> Option<pulse::channelmap::Map> {
        let mut map: pulse::channelmap::Map = Default::default();
        map.init();
        map.set_len(channels.count() as u8);

        let is_mono = channels.count() == 1;

        for (i, channel) in channels.iter().enumerate() {
            map.get_mut()[i] = match channel {
                Channels::FRONT_LEFT if is_mono => pulse::channelmap::Position::Mono,
                Channels::FRONT_LEFT => pulse::channelmap::Position::FrontLeft,
                Channels::FRONT_RIGHT => pulse::channelmap::Position::FrontRight,
                Channels::FRONT_CENTRE => pulse::channelmap::Position::FrontCenter,
                Channels::REAR_LEFT => pulse::channelmap::Position::RearLeft,
                Channels::REAR_CENTRE => pulse::channelmap::Position::RearCenter,
                Channels::REAR_RIGHT => pulse::channelmap::Position::RearRight,
                Channels::LFE1 => pulse::channelmap::Position::Lfe,
                Channels::FRONT_LEFT_CENTRE => pulse::channelmap::Position::FrontLeftOfCenter,
                Channels::FRONT_RIGHT_CENTRE => pulse::channelmap::Position::FrontRightOfCenter,
                Channels::SIDE_LEFT => pulse::channelmap::Position::SideLeft,
                Channels::SIDE_RIGHT => pulse::channelmap::Position::SideRight,
                Channels::TOP_CENTRE => pulse::channelmap::Position::TopCenter,
                Channels::TOP_FRONT_LEFT => pulse::channelmap::Position::TopFrontLeft,
                Channels::TOP_FRONT_CENTRE => pulse::channelmap::Position::TopFrontCenter,
                Channels::TOP_FRONT_RIGHT => pulse::channelmap::Position::TopFrontRight,
                Channels::TOP_REAR_LEFT => pulse::channelmap::Position::TopRearLeft,
                Channels::TOP_REAR_CENTRE => pulse::channelmap::Position::TopRearCenter,
                Channels::TOP_REAR_RIGHT => pulse::channelmap::Position::TopRearRight,
                _ => {
                    // If a Symphonia channel cannot map to a PulseAudio position then return None
                    // because PulseAudio will not be able to open a stream with invalid channels.
                    warn!("failed to map channel {:?} to output", channel);
                    return None;
                }
            }
        }

        Some(map)
    }

    pub fn init_output(&mut self) {
        let spec = self.spec.unwrap();
        let pa_spec = pulse::sample::Spec {
            format: pulse::sample::Format::FLOAT32NE,
            channels: spec.channels.count() as u8,
            rate: spec.rate,
        };
        assert!(pa_spec.is_valid());

        let pa_ch_map = Player::map_channels_to_pa_channelmap(spec.channels);
        let pa = psimple::Simple::new(
            None,                               // Use default server
            "Symphonia Player",                 // Application name
            pulse::stream::Direction::Playback, // Playback stream
            None,                               // Default playback device
            "Music",                            // Description of the stream
            &pa_spec,                           // Signal specificaiton
            pa_ch_map.as_ref(),                 // Channel map
            None,                               // Custom buffering attributes
        )
        .unwrap();
        self.output = Some(pa)
    }

    fn init_reader(&mut self, path: String) {
        let src = std::fs::File::open(path).expect("failed to open media");
        let mss = MediaSourceStream::new(Box::new(src), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");
        self.reader = Some(probed.format);
    }

    fn init_decoder(&mut self) {
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: false,
            ..Default::default()
        };
        if let Some(reader) = &mut self.reader {
            let track = reader.default_track().unwrap();
            let codec_params = &track.codec_params;
            let mut decoder = symphonia::default::get_codecs()
                .make(&codec_params, &dec_opts)
                .unwrap();
            let packet = reader.next_packet().unwrap();
            // self.decoder = Some(decoder);
            let decoded = decoder.decode(&packet).unwrap();
            let spec = decoded.spec();
            self.spec = Some(*spec);
            self.decoder = Some(decoder);
        };
    }
}
