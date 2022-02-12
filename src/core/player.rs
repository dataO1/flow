use crate::core::player;
use crate::core::reader;
use itertools::Itertools;
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::warn;
use pulse::error::PAErr;
use symphonia::core::audio::{Channels, SignalSpec};
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
    Preview(Box<PreviewBuffer>),
}

type Packet = usize;
#[derive(Copy, Clone, PartialEq)]
pub enum PlayerState {
    Unloaded,
    Paused(Packet),
    Playing(Packet),
    Closed,
}

#[derive(Clone)]
pub struct PreviewBuffer {
    pub buf: Vec<f32>,
}

impl PreviewBuffer {
    /// push packet to internal buffer
    fn push(&mut self, packet: &PacketBuffer) {
        // the number of packages to take the average of
        // this kind of sets the "resolution"
        let chunk_size = 10000;
        // downsample packet
        let mut preview_chunk: Vec<f32> = packet
            .decoded
            .samples()
            .into_iter()
            .chunks(chunk_size)
            .into_iter()
            .map(|chunk| {
                let mut num = 0;
                let mut sum = 0.0;
                for samp in chunk {
                    num += 1;
                    sum += samp;
                }
                sum / num as f32
            })
            .collect();
        self.buf.append(&mut preview_chunk);
    }

    /// length of the internal buffer
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns a downsampled preview version
    pub fn get_preview(&self, window_size: usize) -> &[f32] {
        let buf_len = self.len();
        // if buf_len - window size < 0 then left bound is 0
        let l = std::cmp::max(0, buf_len - window_size);
        // println!("l:{}, r:{}", l, buf_len);
        &self.buf[l..buf_len]
    }
}

impl Default for PreviewBuffer {
    fn default() -> Self {
        Self {
            buf: vec![0.0; 200],
        }
    }
}

pub struct Player {
    /// decoded packets
    sample_buffer: Vec<PacketBuffer>,
    /// preview buffer
    preview_buffer: Box<PreviewBuffer>,
    /// player state
    state: PlayerState,
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
    ) -> JoinHandle<()> {
        // The async channel for Events from the reader
        let (reader_message_out, reader_message_rx) = channel::<reader::Message>(1000);
        let (reader_event_tx, reader_event_in) = channel::<reader::Event>(1000);
        let reader_handle = Reader::spawn(reader_event_tx, reader_message_rx);
        // Start the command handler thread
        tokio::spawn(async move {
            let mut player = Player::new(reader_handle);
            player
                .event_loop(
                    player_message_in,
                    player_event_out,
                    reader_message_out,
                    reader_event_in,
                )
                .await
        })
    }

    fn new(reader_handle: JoinHandle<()>) -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        let decoded_packets = vec![];
        Self {
            sample_buffer: decoded_packets,
            state: PlayerState::Unloaded,
            preview_buffer: Box::new(PreviewBuffer::default()),
            // audio_output: None,
        }
    }

    async fn event_loop(
        &mut self,
        mut player_message_in: Receiver<Message>,
        player_event_out: Sender<player::Event>,
        reader_message_out: Sender<reader::Message>,
        mut reader_event_in: Receiver<reader::Event>,
    ) {
        let mut audio_output = None;
        while self.state != PlayerState::Closed {
            // command handlers
            match reader_event_in.try_recv() {
                //------------------------------------------------------------------//
                //                         Reader Messages                          //
                //------------------------------------------------------------------//
                Ok(reader::Event::Init(spec)) => {
                    // We got the specs, so initiate the audio output
                    let pa = Player::get_output(spec);
                    audio_output.replace(pa);
                    self.load();
                }
                Ok(reader::Event::ReaderDone) => {
                    //TODO: close the thread accordingly
                    println!("reader done received");
                }
                Ok(reader::Event::PacketDecoded(packet)) => {
                    // println!("received: {:#?}", &packet.frames);
                    self.preview_buffer.push(&packet);
                    self.sample_buffer.push(packet);
                }
                Err(_) => { //
                }
            };
            match player_message_in.try_recv() {
                //------------------------------------------------------------------//
                //                           App Messages                           //
                //------------------------------------------------------------------//
                Ok(Message::Load(path)) => {
                    // Communicate to the reader, that we want to load a track
                    reader_message_out.send(reader::Message::Load(path)).await;
                }
                Ok(Message::TogglePlay) => {
                    self.toggle_play(&audio_output);
                }
                Ok(Message::Close) => break,
                Ok(msg) => {}
                Err(_) => {
                    // This happens, when there are still outstanding channels, but the message
                    // queue is empty, so just ignore this
                }
            }
            // play buffered packets
            if let PlayerState::Playing(pos) = self.state {
                if let Some(out) = &audio_output {
                    match self.play_buffer(out, pos) {
                        Ok(()) => {
                            player_event_out
                                .send(player::Event::Preview(self.preview_buffer.clone()))
                                .await;
                        }
                        Err(err) => {}
                    }
                }
            }
        }
    }
    fn load(&mut self) {
        self.state = PlayerState::Paused(0);
    }

    fn pause(&mut self, out: &psimple::Simple) {
        out.flush();
    }

    fn toggle_play(&mut self, audio_output: &Option<psimple::Simple>) {
        // check if audio output is valid
        if let Some(out) = &audio_output {
            match self.state {
                PlayerState::Paused(x) => self.state = PlayerState::Playing(x),
                PlayerState::Playing(x) => {
                    self.state = PlayerState::Paused(x);
                    self.pause(out);
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

    fn play_buffer(&mut self, out: &psimple::Simple, pos: usize) -> Result<(), PAErr> {
        self.state = PlayerState::Playing(pos + 1);
        out.write(self.sample_buffer[pos].raw.as_bytes())
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

    pub fn get_output(spec: SignalSpec) -> psimple::Simple {
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
        pa
    }

    fn get_wave_preview(&self, pos: usize) -> Vec<f32> {
        let preview_buff_size = 1;
        let left_bound = if pos < preview_buff_size {
            0
        } else {
            preview_buff_size
        };
        let right_bound = std::cmp::min(pos + preview_buff_size, self.sample_buffer.len());
        // here, we need a SampleBuffer now a RawSampleBuffer, which doesnt have the .samples()
        // method
        let mut preview_buffer = vec![];
        for packet in self.sample_buffer[left_bound..right_bound].into_iter() {
            preview_buffer.extend_from_slice(packet.decoded.samples());
        }
        preview_buffer
    }
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}
