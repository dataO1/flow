use crate::core::reader;
use crate::view::app;
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;

use log::warn;
use symphonia::core::audio::{Channels, RawSampleBuffer, SignalSpec};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

use super::reader::Reader;

pub enum Message {
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
    /// New incoming decoded package
    PacketDecoded(RawSampleBuffer<f32>),
    /// Get specification and duration of audio
    Init((SignalSpec, u64)),
    /// The reader is Done
    ReaderDone,
}
type Packet = usize;
#[derive(Copy, Clone, PartialEq)]
pub enum PlayerState {
    Unloaded,
    Loaded,
    Playing(Packet),
    Closed,
}

pub struct Player {
    /// Sender for the app thread
    app_channel: Sender<app::Event>,
    /// handle for the reader thread
    reader_handle: JoinHandle<()>,
    /// decoded packets
    sample_buffer: Vec<RawSampleBuffer<f32>>,
    /// player state
    state: PlayerState,
}

impl Player {
    //------------------------------------------------------------------//
    //                          Public Methods                          //
    //------------------------------------------------------------------//

    /// Initializes a new thread, that handles Commands.
    /// Returns a Sender, which can be used to send messages to the player
    pub fn spawn(app_channel: Sender<app::Event>) -> Sender<Message> {
        // The async channels for Messages to the player
        let (player_tx, mut player_rx) = channel::<Message>(1000);
        // The async channel for Events from the reader
        let (reader_tx, mut reader_rx) = channel::<reader::Message>(1000);
        let reader_handle = Reader::spawn(player_tx.clone(), reader_rx);
        // Start the command handler thread
        let player_handle = tokio::spawn(async move {
            let mut player = Player::new(app_channel, reader_handle);
            player.event_loop(player_rx, reader_tx).await
        });
        player_tx.clone()
    }

    //------------------------------------------------------------------//
    //                         Command Handlers                         //
    //------------------------------------------------------------------//
    fn new(app: Sender<app::Event>, reader_handle: JoinHandle<()>) -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        let decoded_packets = vec![];
        Self {
            app_channel: app,
            reader_handle,
            sample_buffer: decoded_packets,
            state: PlayerState::Unloaded,
            // audio_output: None,
        }
    }

    async fn event_loop(&mut self, mut input: Receiver<Message>, reader: Sender<reader::Message>) {
        let mut audio_output = None;
        while self.state != PlayerState::Closed {
            // command handlers
            match input.try_recv() {
                //------------------------------------------------------------------//
                //                         Reader Messages                          //
                //------------------------------------------------------------------//
                Ok(Message::Init((spec, duration))) => {
                    // We got the specs, so initiate the audio output
                    let pa = Player::get_output(spec, duration);
                    audio_output.replace(pa);
                }
                Ok(Message::ReaderDone) => {
                    //TODO: close the thread accordingly
                    println!("reader done received");
                }
                //------------------------------------------------------------------//
                //                           App Messages                           //
                //------------------------------------------------------------------//
                Ok(Message::Load(path)) => {
                    // Communicate to the reader, that we want to load a track
                    reader.send(reader::Message::Load(path)).await;
                }
                Ok(Message::TogglePlay) => {
                    self.toggle_play(&audio_output);
                }
                Ok(Message::PacketDecoded(packet)) => {
                    // println!("received: {:#?}", &packet.frames);
                    self.sample_buffer.push(packet);
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
                    self.play(out, pos);
                }
            }
        }
    }
    fn play(&mut self, out: &psimple::Simple, pos: usize) {
        out.write(self.sample_buffer[pos].as_bytes());
        self.state = PlayerState::Playing(pos + 1);
    }
    fn pause(&mut self, out: &psimple::Simple) {
        out.flush();
    }
    fn toggle_play(&mut self, audio_output: &Option<psimple::Simple>) {
        if self.state == PlayerState::Loaded {
            self.state = PlayerState::Playing(0);
        } else {
            self.state = PlayerState::Loaded;
            if let Some(out) = &audio_output {
                self.pause(out);
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

    pub fn get_output(spec: SignalSpec, duration: u64) -> psimple::Simple {
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
