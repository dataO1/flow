use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};

use crate::core::player;
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::warn;
use std::sync::mpsc::{Receiver, Sender};
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::audio::{Channels, SignalSpec};
use symphonia::core::codecs::Decoder;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatReader;
use symphonia::core::formats::{FormatOptions, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase, TimeStamp};

pub enum SkipType {
    Forward,
    Backward,
}

pub enum Message {
    /// Load a new file
    Load(String),
    /// Toggle playback
    TogglePlay,
    /// Same as Cue button on CDJ
    Cue,
    /// Skip forward a number of millis
    SkipForward(Time),
    /// Skip backwards a number of millis
    SkipBackward(Time),
    /// Get missing preview Data. The parameter tells the player how many preview samples the app
    /// already has
    GetPreview(usize),
}

pub enum Event {}

#[derive(Copy, Clone, PartialEq)]
pub enum PlayerState {
    Unloaded,
    Paused,
    Playing,
    Closed,
}

/// struct for converting between different formats for marking a specific time in a track
#[derive(Clone)]
pub struct TimeMarker {
    /// everything is stored in timestamp format
    ts: TimeStamp,
    /// the tracks timebase, needed for conversion to and from TimeStamp format
    track: Track,
}

impl TimeMarker {
    pub fn new(track: Track) -> Self {
        Self { track, ts: 0 }
    }

    fn add_time(&mut self, offset: Time) {
        let mut current = self
            .track
            .codec_params
            .time_base
            .unwrap()
            .calc_time(self.ts);
        let new_time = Time {
            seconds: (current.seconds + offset.seconds),
            frac: (current.frac + offset.frac),
        };
        let new_ts = self
            .track
            .codec_params
            .time_base
            .unwrap()
            .calc_timestamp(new_time);
        self.ts = new_ts;
    }

    fn go_to(&mut self, ts: u64) {
        self.ts = ts;
    }

    pub fn get_timestamp(&self) -> u64 {
        self.ts
    }

    pub fn get_time_in_seconds(&self) -> f64 {
        let time = self
            .track
            .codec_params
            .time_base
            .unwrap()
            .calc_time(self.ts);
        (time.seconds as f64) + (time.frac)
    }
}

pub struct Player {
    /// player state
    state: PlayerState,
    /// current playhead position
    position_marker: Arc<Mutex<Option<TimeMarker>>>,
    /// cue point as a TimeMarker
    cue_point_marker: Option<TimeMarker>,
    /// Formatreader
    reader: Option<Box<dyn FormatReader>>,
    /// Decoder
    decoder: Option<Box<dyn Decoder>>,
    /// PulseAudio output
    output: Option<psimple::Simple>,
    /// Signal Spec
    spec: Option<SignalSpec>,
    /// Symphonia track information
    track: Option<Track>,
}

impl Player {
    //------------------------------------------------------------------//
    //                          Public Methods                          //
    //------------------------------------------------------------------//

    /// Initializes a new thread, that handles Commands.
    /// Returns a Sender, which can be used to send messages to the player
    pub fn spawn(
        player_position: Arc<Mutex<Option<TimeMarker>>>,
        player_message_in: Receiver<player::Message>,
        player_event_out: Sender<player::Event>,
    ) -> JoinHandle<()> {
        // The async channel for Events from the reader
        // Start the command handler thread
        spawn(move || {
            let mut player = Player::new(player_position);
            player.event_loop(player_message_in, player_event_out)
        })
    }

    fn new(position: Arc<Mutex<Option<TimeMarker>>>) -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        Self {
            state: PlayerState::Unloaded,
            reader: None,
            decoder: None,
            output: None,
            spec: None,
            track: None,
            cue_point_marker: None,
            position_marker: position,
        }
    }

    fn event_loop(
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
                Ok(Message::Cue) => {
                    self.cue();
                }
                Ok(Message::SkipForward(time)) => {
                    self.skip(time, SkipType::Forward);
                }
                Ok(Message::SkipBackward(time)) => {
                    self.skip(time, SkipType::Backward);
                }
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
        if let Some(track) = &self.track {
            *self.position_marker.lock().unwrap() = Some(TimeMarker::new(track.clone()));
            self.cue_point_marker = (*self.position_marker.lock().unwrap()).clone();
        }
    }

    fn cue(&mut self) {
        if self.state != PlayerState::Playing {
            // set cue new point
            self.cue_point_marker = (*self.position_marker.lock().unwrap()).clone();
        } else {
            // return to last cue point
            if let (Some(track), Some(reader), Some(cue)) =
                (&self.track, &mut self.reader, &self.cue_point_marker)
            {
                let sample_rate = track.codec_params.sample_rate.unwrap();
                *self.position_marker.lock().unwrap() = self.cue_point_marker.clone();
                reader.seek(
                    symphonia::core::formats::SeekMode::Accurate,
                    symphonia::core::formats::SeekTo::TimeStamp {
                        ts: cue.ts,
                        track_id: track.id,
                    },
                );
            }
        }
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
                PlayerState::Paused => {
                    self.state = PlayerState::Playing;
                }
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

    /// skip a given amount of milliseconds, either forward or backwards
    fn skip(&mut self, offset: Time, t: SkipType) {
        if let (Some(track), Some(reader), Some(playhead)) = (
            &self.track,
            &mut self.reader,
            &mut (*self.position_marker.lock().unwrap()),
        ) {
            playhead.add_time(offset);
            let track_id = track.id;
            let res = reader.seek(
                symphonia::core::formats::SeekMode::Accurate,
                symphonia::core::formats::SeekTo::TimeStamp {
                    ts: playhead.ts,
                    track_id,
                },
            );
        }
    }

    fn play(&mut self) -> Result<(), symphonia::core::errors::Error> {
        match (
            &mut self.reader,
            &mut self.decoder,
            &mut self.output,
            &self.track,
        ) {
            (Some(reader), Some(decoder), Some(out), Some(track)) => {
                let packet = reader.next_packet()?;
                if let Some(pos) = &mut (*self.position_marker.lock().unwrap()) {
                    pos.go_to(packet.ts());
                }
                let decoded = decoder.decode(&packet).unwrap();
                let mut raw_sample_buf =
                    RawSampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                raw_sample_buf.copy_interleaved_ref(decoded);
                match out.write(raw_sample_buf.as_bytes()) {
                    Ok(_) => {
                        Ok(())
                        // successfully wrote buffer
                        // println!("success");
                    }
                    Err(err) => {
                        panic!("Failed to write to output device");
                        // PAErr
                        // println!("Error: {}", err);
                    }
                }
            }
            _ => {
                panic!("Not everything was initialized");
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
            if let None = self.track {
                self.track = Some(track.clone());
            }
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
