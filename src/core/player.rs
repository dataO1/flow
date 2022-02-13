use std::sync::Arc;
use std::sync::Mutex;

use crate::core::player;
use crate::core::reader;
use itertools::Itertools;
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::warn;
use pulse::error::PAErr;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::audio::{Channels, SignalSpec};
use symphonia::core::codecs::Decoder;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::FormatReader;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

use super::reader::PacketBuffer;
use super::reader::Reader;

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
    /// Played x messages
    PlayedPackage(usize),
}

#[derive(Copy, Clone, PartialEq)]
pub enum PlayerState {
    Unloaded,
    Paused,
    Playing,
    Closed,
}

pub struct PreviewBuffer {
    /// A downsampled version of the raw packets. 1 Packet = 1 preview sample
    buf: Vec<f32>,
    /// determines the number of samples for preview per packet. Since samples are interleaved,
    /// this should be a multiple of the number of channels (usually two for stereo)
    samples_per_packet: usize,
}

impl PreviewBuffer {
    /// push packet to internal buffer
    fn push(&mut self, packet: &PacketBuffer) {
        // downsample packet
        let samples = packet.decoded.samples();
        // since the samples in the packets are interlaeved (2 channels), we have to adjust the
        // chunk size
        let chunk_size = samples.len() / (self.samples_per_packet);
        let mut preview_samples: Vec<f32> = samples
            .into_iter()
            .chunks(chunk_size)
            .into_iter()
            .map(|chunk| {
                let mut num = 0;
                let mut sum: f32 = 0.0;
                for sample in chunk {
                    num += 1;
                    sum += sample;
                }
                sum / num as f32
            })
            .collect();
        self.buf.append(&mut preview_samples);
    }

    /// length of the internal buffer
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns a downsampled preview version
    pub fn get_live_preview(
        &self,
        target_size: usize,
        player_pos: usize,
        playhead_position: usize,
    ) -> Vec<f32> {
        let player_pos = player_pos * self.samples_per_packet;
        // check if enough sampes exist for target resolution
        let diff = player_pos as isize - (target_size as isize / 2);
        if diff >= 0 {
            // if yes return buffer content
            let l = player_pos as f32 - (target_size as f32 / 2.0);
            let r = player_pos as f32 + (target_size as f32 / 2.0);
            self.buf[l as usize..r as usize].to_owned()
        } else {
            let diff = diff.abs() as usize;
            let mut padding = vec![0.0 as f32; diff];
            padding.append(&mut self.buf.to_vec());
            padding.to_owned()
        }
    }

    pub fn get_preview(&self, target_size: usize) -> Vec<f32> {
        if target_size > self.len() {
            vec![0.0; target_size]
        } else {
            let chunk_size = (self.len() as f32 / target_size as f32).floor() as usize;
            let preview: Vec<f32> = self
                .buf
                .to_owned()
                .into_iter()
                .chunks(chunk_size)
                .into_iter()
                .map(|chunk| {
                    let mut sum: f32 = 0.0;
                    let mut num = 0;
                    for packet in chunk {
                        num += 1;
                        sum += packet;
                    }
                    sum / num as f32
                })
                .collect();
            preview
        }
    }
}

impl Default for PreviewBuffer {
    fn default() -> Self {
        Self {
            buf: vec![],
            samples_per_packet: 2 << 2,
            // player_pos: 0,
        }
    }
}

pub struct Player {
    /// player state
    state: PlayerState,
    /// player position in packages
    position: usize,
    reader: Option<Box<dyn FormatReader>>,
    decoder: Option<Box<dyn Decoder>>,
    output: Option<psimple::Simple>,
    spec: Option<SignalSpec>,
}

impl Player {
    //------------------------------------------------------------------//
    //                          Public Methods                          //
    //------------------------------------------------------------------//

    /// Initializes a new thread, that handles Commands.
    /// Returns a Sender, which can be used to send messages to the player
    pub fn spawn(
        player_message_in: Receiver<player::Message>,
        player_event_out: Sender<player::Event>,
        frame_buffer: Arc<Mutex<PreviewBuffer>>,
    ) -> JoinHandle<()> {
        // The async channel for Events from the reader
        // Start the command handler thread
        tokio::spawn(async move {
            let mut player = Player::new();
            player.event_loop(player_message_in, player_event_out).await
        })
    }

    fn new() -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        Self {
            state: PlayerState::Unloaded,
            position: 0,
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
                    self.state = PlayerState::Paused;
                    self.init_reader(path);
                    self.init_decoder();
                    self.init_output();
                }
                Ok(Message::TogglePlay) => {
                    self.toggle_play();
                }
                Ok(Message::Close) => break,
                Ok(msg) => {}
                Err(_) => {
                    // This happens, when there are still outstanding channels, but the message
                    // queue is empty, so just ignore this
                }
            }
            // play buffered packets
            if let PlayerState::Playing = self.state {
                if let Some(out) = &mut self.output {
                    self.play();
                    player_event_out.send(player::Event::PlayedPackage(1)).await;
                }
            }
        }
    }
    fn load(&mut self) {
        self.state = PlayerState::Paused;
    }

    fn pause(&mut self) {
        if let Some(out) = &mut self.output {
            out.flush();
        }
    }

    fn toggle_play(&mut self) {
        // check if audio output is valid
        if let Some(out) = &mut self.output {
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
        self.position += 1;
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

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}
