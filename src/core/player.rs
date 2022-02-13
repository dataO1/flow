use std::sync::Arc;
use std::sync::Mutex;

use crate::core::player;
use crate::core::reader;
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::warn;
use pulse::error::PAErr;
use symphonia::core::audio::RawSampleBuffer;
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
    // Preview(Box<PreviewBuffer>),
}

#[derive(Copy, Clone, PartialEq)]
pub enum PlayerState {
    Unloaded,
    Paused,
    Playing,
    Closed,
}

pub struct FrameBuffer {
    /// Original Source Packets
    pub packets: Vec<PacketBuffer>,
    /// A downsampled version of the raw packets. 1 Packet = 1 preview sample
    preview_buffer: Vec<f32>,
    /// current playing packet of the player
    player_pos: usize,
}

impl FrameBuffer {
    /// push packet to internal buffer
    fn push(&mut self, packet: PacketBuffer) {
        // downsample packet
        let samples = packet.decoded.samples();
        let num_samples = samples.len();
        let sum: f32 = samples.iter().sum();
        let preview_sample = sum / num_samples as f32;
        // self.buf.append(&mut preview_chunk);
        self.packets.push(packet);
        self.preview_buffer.push(preview_sample);
    }

    /// length of the internal buffer
    pub fn len(&self) -> usize {
        self.preview_buffer.len()
    }

    /// Returns a downsampled preview version
    pub fn get_preview(&self, target_resolution: usize) -> Vec<f32> {
        // check if enough sampes exist for target resolution
        let diff = self.len() as isize - target_resolution as isize;
        if diff >= 0 {
            // if yes return buffer content
            let l = self.player_pos as f32 - (target_resolution as f32 / 2.0);
            let r = self.player_pos as f32 + (target_resolution as f32 / 2.0);
            self.preview_buffer[l as usize..r as usize].to_owned()
        } else {
            let diff = diff.abs() as usize;
            let mut padding = vec![0.0 as f32; diff];
            padding.append(&mut self.preview_buffer.to_vec());
            padding.to_owned()
        }
    }

    /// advance the buffer by one packet
    pub fn advance_position(&mut self) {
        self.player_pos += 1;
    }

    pub fn get_curr_raw(&self) -> &RawSampleBuffer<f32> {
        &self.packets[self.player_pos].raw
    }
}

impl Default for FrameBuffer {
    fn default() -> Self {
        Self {
            packets: vec![],
            preview_buffer: vec![],
            player_pos: 0,
        }
    }
}

pub struct Player {
    /// frame buffer
    frame_buffer: Arc<Mutex<FrameBuffer>>,
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
        frame_buffer: Arc<Mutex<FrameBuffer>>,
    ) -> JoinHandle<()> {
        // The async channel for Events from the reader
        let (reader_message_out, reader_message_rx) = channel::<reader::Message>(1000);
        let (reader_event_tx, reader_event_in) = channel::<reader::Event>(1000);
        let reader_handle = Reader::spawn(reader_event_tx, reader_message_rx);
        // Start the command handler thread
        tokio::spawn(async move {
            let mut player = Player::new(reader_handle, frame_buffer);
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

    fn new(reader_handle: JoinHandle<()>, preview_buffer: Arc<Mutex<FrameBuffer>>) -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        Self {
            state: PlayerState::Unloaded,
            frame_buffer: preview_buffer,
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
                    self.frame_buffer.lock().unwrap().push(packet);
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
            if let PlayerState::Playing = self.state {
                if let Some(out) = &audio_output {
                    match self.play_buffer(out) {
                        Ok(()) => {
                            // player_event_out
                            //     .send(player::Event::Preview(self.preview_buffer.clone()))
                            //     .await;
                            ()
                        }
                        Err(err) => {}
                    }
                }
            }
        }
    }
    fn load(&mut self) {
        self.state = PlayerState::Paused;
    }

    fn pause(&mut self, out: &psimple::Simple) {
        out.flush();
    }

    fn toggle_play(&mut self, audio_output: &Option<psimple::Simple>) {
        // check if audio output is valid
        if let Some(out) = &audio_output {
            match self.state {
                PlayerState::Paused => self.state = PlayerState::Playing,
                PlayerState::Playing => {
                    self.state = PlayerState::Paused;
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

    fn play_buffer(&mut self, out: &psimple::Simple) -> Result<(), PAErr> {
        let mut frame_buffer = self.frame_buffer.lock().unwrap();
        frame_buffer.advance_position();
        out.write(frame_buffer.get_curr_raw().as_bytes())
        // out.drain()
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
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}
