use crate::core::analyzer;
use crate::view::model;
use std::sync::{Arc, Mutex};

use itertools::Itertools;
use log::warn;

use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CodecParameters, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader, Track},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};
use tokio::{sync::mpsc::Sender, task::JoinHandle};

use super::player::PreviewBuffer;

//------------------------------------------------------------------//
//                             Analyzer                             //
//------------------------------------------------------------------//
const MAX_CACHE_SIZE: usize = 1000;
/// determines the number of samples in the preview buffer per packet of the original source
pub const PREVIEW_SAMPLES_PER_PACKET: usize = 2 << 3;

#[derive(Debug)]
pub enum AnalyzerError {
    ReaderError,
    UnsupportedFormat,
    NoTrackFound,
}

pub enum Event {
    /// This event fires, when a analyzer is done analyzing
    DoneAnalyzing(String),
    NewTrack(Arc<model::track::Track>),
}

pub struct Analyzer {
    /// The track to be analyzed
    track: Arc<model::track::Track>,
    /// FormatReader
    reader: Box<dyn FormatReader>,
    /// Decoder
    decoder: Box<dyn Decoder>,
    /// Local Cache for analyzed samples
    sample_cache: Vec<f32>,
    /// analyzer event sender
    analyzer_event_out: Sender<Event>,
}

impl Analyzer {
    pub fn spawn(file_path: String, analyzer_event_out: Sender<analyzer::Event>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut analyzer = Analyzer::new(file_path.clone(), analyzer_event_out).await;
            // messages
            loop {
                match analyzer.decode() {
                    Ok(samples) => analyzer.analyze_samples(samples).await,
                    Err(_) => {
                        // Error decoding
                        // this means the stream is done?
                        analyzer
                            .analyzer_event_out
                            .send(analyzer::Event::DoneAnalyzing(file_path))
                            .await;
                        break;
                    }
                }
            }
        })
    }

    async fn new(file_path: String, analyzer_event_out: Sender<analyzer::Event>) -> Self {
        let mut reader = Analyzer::get_reader(file_path.clone());
        let codec_params = reader.default_track().unwrap().clone().codec_params;
        let decoder = Analyzer::get_decoder(&codec_params, &mut reader).unwrap();
        let track = Arc::new(model::track::Track::new(file_path, codec_params));
        analyzer_event_out
            .send(Event::NewTrack(Arc::clone(&track)))
            .await;
        Self {
            reader,
            decoder,
            sample_cache: vec![],
            track,
            analyzer_event_out,
        }
    }

    // creates a new @FormatReader

    fn decode(&mut self) -> Result<SampleBuffer<f32>, Error> {
        let packet = self.reader.next_packet()?;
        match self.decoder.decode(&packet) {
            Ok(decoded) => {
                // Get the audio buffer specification. This is a description of the decoded
                // audio buffer's sample format and sample rate.
                let spec = *decoded.spec();

                // Get the capacity of the decoded buffer. Note that this is capacity, not
                // length! The capacity of the decoded buffer is constant for the life of the
                // decoder, but the length is not.
                let duration = decoded.capacity() as u64;
                let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
                sample_buf.copy_interleaved_ref(decoded.clone());
                Ok(sample_buf)
            }
            Err(err) => {
                // Decode errors are not fatal. Print the error message and try to decode the next
                // packet as usual.
                warn!("decode error: {}", err);
                panic!("error")
            }
        }
    }

    fn get_reader(path: String) -> Box<dyn FormatReader> {
        let src = std::fs::File::open(path).expect("failed to open media");
        let mss = MediaSourceStream::new(Box::new(src), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");
        probed.format
    }

    fn get_decoder(
        codec_params: &CodecParameters,
        reader: &mut Box<dyn FormatReader>,
    ) -> Result<Box<dyn Decoder>, AnalyzerError> {
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: false,
            ..Default::default()
        };
        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &dec_opts)
            .unwrap();
        let packet = reader.next_packet().unwrap();
        // self.decoder = Some(decoder);
        let decoded = decoder.decode(&packet).unwrap();
        Ok(decoder)
    }

    async fn analyze_samples(&mut self, sample_buffer: SampleBuffer<f32>) {
        self.sample_cache.extend_from_slice(sample_buffer.samples());
        // as soon as we have enough cached samples send them to the app
        if self.sample_cache.len() >= MAX_CACHE_SIZE {
            let mut downsampled = self.downsample_to_preview(&self.sample_cache);
            self.track.append(&mut downsampled);
        }
    }

    fn downsample_to_preview(&self, samples: &Vec<f32>) -> Vec<f32> {
        let chunk_size = samples.len() / PREVIEW_SAMPLES_PER_PACKET;
        let mut preview_samples = samples
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
        preview_samples
    }
}
