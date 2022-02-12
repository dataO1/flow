use crate::core::{output, reader};
use crate::Event;
use log::warn;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::{FormatOptions, FormatReader, Packet, SeekMode, SeekTo};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use symphonia::core::{formats::Track, io::MediaSourceStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

use super::reader::{DecodedPacket, Reader};

pub enum PlayerMessage {
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
    Decoded(Box<DecodedPacket>),
}

pub struct Player {
    /// Sender for the app thread
    app_channel: Sender<Event>,
    /// handle for the reader thread
    reader_handle: JoinHandle<()>,
    /// decoded packets
    decoded_packets: Vec<Box<DecodedPacket>>,
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
    /// Returns a Sender, which can be used to send messages to the player
    pub fn spawn(app: Sender<Event>) -> Sender<PlayerMessage> {
        // The async channels for Messages to the player
        let (player_tx, mut player_rx) = channel::<PlayerMessage>(1000);
        // The async channel for Events from the reader
        let (reader_tx, mut reader_rx) = channel::<reader::Message>(1000);
        let reader_handle = Reader::spawn(player_tx.clone(), reader_rx);
        // Start the command handler thread
        let player_handle = tokio::spawn(async move {
            let mut player = Player::new(app, reader_handle);
            player.event_loop(&mut player_rx, reader_tx).await
        });
        player_tx
    }

    //------------------------------------------------------------------//
    //                         Command Handlers                         //
    //------------------------------------------------------------------//
    fn new(app: Sender<Event>, reader_handle: JoinHandle<()>) -> Self {
        // the frame buffer. TODO: use sensible vector sizes
        let decoded_packets = vec![];
        Self {
            app_channel: app,
            reader_handle,
            decoded_packets,
            // audio_output: None,
        }
    }

    async fn event_loop(&mut self, rx: &mut Receiver<PlayerMessage>, tx: Sender<reader::Message>) {
        // Async event handlers here:
        loop {
            // command handlers
            match rx.try_recv() {
                Ok(PlayerMessage::Load(path)) => {
                    tx.send(reader::Message::Load(path)).await;
                }
                Ok(PlayerMessage::TogglePlay) => {}
                Ok(PlayerMessage::Decoded(packet)) => {
                    // println!("received: {:#?}", &packet.frames);
                    self.decoded_packets.push(packet);
                }
                Ok(PlayerMessage::Close) => break,
                Ok(msg) => {}
                Err(_) => {
                    // This happens, when there are still outstanding channels, but the message
                    // queue is empty, so just ignore this
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}
