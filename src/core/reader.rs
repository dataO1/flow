use crate::core::{player, reader};
use log::warn;
use symphonia::core::{
    audio::{AudioBufferRef, RawSampleBuffer, SampleBuffer, SignalSpec},
    codecs::{Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader, Packet},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
};

//------------------------------------------------------------------//
//                              READER                              //
//------------------------------------------------------------------//

#[derive(Copy, Clone, PartialEq)]
enum ReaderState {
    Initializing,
    Loading(u32),
    Finished,
}

#[derive(Debug)]
pub enum Message {
    Load(String),
    Exit,
}
pub enum Event {
    /// New incoming decoded package
    PacketDecoded(RawSampleBuffer<f32>),
    /// Get specification and duration of audio
    Init((SignalSpec, u64)),
    /// The reader is Done
    ReaderDone,
}

pub struct Reader {
    state: ReaderState,
}

impl Reader {
    pub fn spawn(
        player_out: Sender<reader::Event>,
        mut reader_message: Receiver<Message>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut reader = Box::new(Reader::default());
            let mut format_reader = None;
            let mut decoder = None;
            let mut sent_spec = false;
            // messages
            while !reader.is_done() {
                if let Ok(msg) = reader_message.try_recv() {
                    println!("reader got message");
                    match msg {
                        Message::Load(file_path) => {
                            println!("got load message");
                            reader.state = ReaderState::Loading(0);
                            format_reader = Some(Reader::get_reader(&file_path));
                            if let Some(r) = &mut format_reader {
                                decoder = match Reader::get_decoder(r) {
                                    Ok(dec) => Some(dec),
                                    Err(_) => todo!(),
                                };
                            };
                        }
                        Message::Exit => return,
                    }
                }
                if let (ReaderState::Loading(_), Some(d), Some(r)) =
                    (reader.state, &mut decoder, &mut format_reader)
                {
                    // loading loop
                    match reader.next_packet(r, d) {
                        Ok((sample_buff, spec, duration)) => {
                            // println!("decoded a packet");
                            if !sent_spec {
                                sent_spec = true;
                                player_out.send(reader::Event::Init((spec, duration))).await;
                                println!("sent spec");
                            }
                            player_out
                                .send(reader::Event::PacketDecoded(sample_buff))
                                .await;
                        }
                        Err(err) => {
                            println!("{:#?}", err);
                        }
                    }
                };
            }
            // events
        })
    }

    fn is_done(&self) -> bool {
        self.state == ReaderState::Finished
    }

    // creates a new @FormatReader
    fn get_reader(file_path: &str) -> Box<dyn FormatReader> {
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

    fn get_decoder(reader: &Box<dyn FormatReader>) -> Result<Box<dyn Decoder>, Error> {
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: true,
            ..Default::default()
        };
        // select the first track with a known codec.
        //
        let track = reader.default_track();
        let codec_params = &track.unwrap().codec_params;
        let decoder = symphonia::default::get_codecs().make(&codec_params, &dec_opts);
        decoder
    }

    fn next_packet(
        &self,
        reader: &mut Box<dyn FormatReader>,
        decoder: &mut Box<dyn Decoder>,
    ) -> Result<(RawSampleBuffer<f32>, SignalSpec, u64), Error> {
        let packet = reader.next_packet()?;
        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Get the audio buffer specification. This is a description of the decoded
                // audio buffer's sample format and sample rate.
                let spec = *decoded.spec();

                // Get the capacity of the decoded buffer. Note that this is capacity, not
                // length! The capacity of the decoded buffer is constant for the life of the
                // decoder, but the length is not.
                let duration = decoded.capacity() as u64;
                let mut sample_buf = RawSampleBuffer::<f32>::new(duration, spec);
                sample_buf.copy_interleaved_ref(decoded.clone());
                // let samples = sample_buf.samples().to_owned();
                // if audio_output.is_none() {
                //
                //     // Try to open the audio output.
                //     audio_output.replace(output::try_open(spec, duration).unwrap());
                //     // create sample buffer
                // } else {
                //     // TODO: Check the audio spec. and duration hasn't changed.
                // }
                // Write the decoded audio samples to the audio output if the presentation timestamp
                // for the packet is >= the seeked position (0 if not seeking).
                // if packet.ts() >= play_opts.seek_ts {
                //     // print_progress(packet.ts(), dur, tb); //TODO: print progress
                //
                //     if let Some(audio_output) = audio_output {
                //         audio_output.write(decoded.clone()).unwrap()
                //     }
                // }
                Ok((sample_buf, spec, duration))
            }
            Err(err) => {
                // Decode errors are not fatal. Print the error message and try to decode the next
                // packet as usual.
                warn!("decode error: {}", err);
                panic!("error")
            }
        }
    }
}

impl Default for Reader {
    fn default() -> Reader {
        Reader {
            state: ReaderState::Initializing,
        }
    }
}
