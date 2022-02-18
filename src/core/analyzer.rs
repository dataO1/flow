use crate::core::analyzer;
use crate::view::model;
use samplerate::{ConverterType, Samplerate};
use std::{
    sync::Arc,
    thread::{spawn, JoinHandle},
    time::Duration,
};
use synthrs::filter::{
    bandpass_filter, convolve, cutoff_from_frequency, highpass_filter, lowpass_filter,
};

use itertools::Itertools;
use log::warn;

use std::sync::mpsc::Sender;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CodecParameters, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

//------------------------------------------------------------------//
//                             Analyzer                             //
//------------------------------------------------------------------//
/// Max number of preview samples to cache before sending to
/// the shared preview buffer of the track
const PREVIEW_CACHE_MAX: usize = 1000;
/// Determines the number of samples in the preview buffer per packet of the original source.
/// Should be a multiple of number of channels
pub const PREVIEW_SAMPLES_PER_PACKET: usize = 6;

/// This is a mono-summed, downsampled version of a number of decoded samples
#[derive(Copy, Clone, Debug)]
pub struct PreviewSample {
    pub lows: f32,
    pub mids: f32,
    pub highs: f32,
}

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
    /// analyzer event sender
    analyzer_event_out: Sender<Event>,
    /// The track to be analyzed
    track: Arc<model::track::Track>,
    /// FormatReader
    reader: Box<dyn FormatReader>,
    /// Decoder
    decoder: Box<dyn Decoder>,
    /// Local Cache for analyzed samples
    sample_buf: Vec<f32>,
    /// Local Cache for downsampled samples
    preview_buf: Vec<f32>,
    beats: Vec<f32>,
}

impl Analyzer {
    pub fn spawn(file_path: String, analyzer_event_out: Sender<analyzer::Event>) -> JoinHandle<()> {
        spawn(move || {
            let mut analyzer = Analyzer::new(file_path.clone(), analyzer_event_out);
            // messages
            loop {
                match analyzer.decode() {
                    Ok(packet) => {
                        analyzer.analyze_packet(packet);
                    }
                    Err(_) => {
                        // Error decoding
                        // this means the stream is done?
                        analyzer
                            .analyzer_event_out
                            .send(analyzer::Event::DoneAnalyzing(file_path))
                            .unwrap();
                        println!("{:#?}", analyzer.beats.len());
                        analyzer.analyze_bpm();
                        break;
                    }
                }
            }
        })
    }

    fn new(file_path: String, analyzer_event_out: Sender<analyzer::Event>) -> Self {
        let reader = Analyzer::get_reader(file_path.clone());
        let codec_params = reader.default_track().unwrap().clone().codec_params;
        let decoder = Analyzer::get_decoder(&codec_params).unwrap();
        let track = Arc::new(model::track::Track::new(file_path, codec_params));
        analyzer_event_out
            .send(Event::NewTrack(Arc::clone(&track)))
            .unwrap();
        Self {
            reader,
            decoder,
            sample_buf: vec![],
            preview_buf: vec![],
            track,
            analyzer_event_out,
            beats: vec![],
        }
    }

    /// returns a sample buffer, that contains one packet of samples in decoded, interleaved form
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
                // store sample data in interleaved format
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

    /// creates reader from a given path
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

    /// creates decoder from codec parameters
    fn get_decoder(codec_params: &CodecParameters) -> Result<Box<dyn Decoder>, AnalyzerError> {
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: false,
            ..Default::default()
        };
        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &dec_opts)
            .unwrap();
        Ok(decoder)
    }

    /// analyze a decoded packet
    fn analyze_packet(&mut self, sample_buffer: SampleBuffer<f32>) {
        // this is the interleaved sample buffer, which means for each point in time there are n
        // samples where n is the number of channels in the track (for stereo that's 2)
        let samples = sample_buffer.samples();
        self.track.set_estimated_samples_per_packet(samples.len());
        // cache decoded frames
        self.sample_buf.extend_from_slice(samples);
        let num_channels = self.track.codec_params.channels.unwrap().count();
        let converter =
            Samplerate::new(ConverterType::SincFastest, 44100, 441, num_channels).unwrap();
        let samples = converter.process_last(samples).unwrap();
        let mut samples =
            Analyzer::downsample_to_preview(&samples, num_channels, PREVIEW_SAMPLES_PER_PACKET);
        assert![samples.len() == PREVIEW_SAMPLES_PER_PACKET];
        self.preview_buf.append(&mut samples);
        // as soon as we have enough cached preview samples send them to the shared buffer of
        // the track
        if self.preview_buf.len() >= PREVIEW_CACHE_MAX {
            // convert cached downsampled buffer to preview samples
            let mut preview_samples = self.samples_2_preview_samples(&self.preview_buf);
            self.track.append_preview_samples(&mut preview_samples);
            self.preview_buf = vec![];
        }
    }

    fn analyze_bpm(&mut self) {
        // analyze bpm
        let hop_s = 512;
        let buf_s = 1024;
        let mut tempo = std::panic::catch_unwind(|| {
            aubio::Tempo::new(
                aubio::OnsetMode::Hfc,
                buf_s,
                hop_s,
                self.track.codec_params.sample_rate.unwrap(),
            )
            .unwrap()
        });
        match tempo {
            Ok(mut tempo) => {
                self.sample_buf
                    .to_vec()
                    .into_iter()
                    .chunks(buf_s)
                    .into_iter()
                    .map(|chunk| {
                        let chunk: Vec<f32> = chunk.into_iter().collect();
                        match tempo.do_result(chunk) {
                            Ok(_) => {}
                            Err(_) => {}
                        };
                    });
                let t = tempo.get_bpm();
                println!("{}", t);
            }
            Err(err) => {
                println!("{:#?}", err);
            }
        };
    }

    /// convert a buffer of samples into a buffer of preview samples of same lenght
    fn samples_2_preview_samples(&self, samples: &Vec<f32>) -> Vec<PreviewSample> {
        let samples = samples.into_iter().map(|s| *s as f64).collect_vec();
        let sample_rate = self.track.codec_params.sample_rate.unwrap() as usize;
        let low_low_crossover = cutoff_from_frequency(20., sample_rate);
        let high_low_crossover = cutoff_from_frequency(100., sample_rate);
        let low_mid_crossover = cutoff_from_frequency(100., sample_rate);
        let high_mid_crossover = cutoff_from_frequency(1000., sample_rate);
        let low_high_crossover = cutoff_from_frequency(5000., sample_rate);
        let high_high_crossover = cutoff_from_frequency(8000., sample_rate);
        let low_band_filter = bandpass_filter(low_low_crossover, high_low_crossover, 0.1);
        let lows = convolve(&low_band_filter, &samples[..]);
        let high_band_filter = bandpass_filter(low_high_crossover, high_high_crossover, 0.1);
        let highs = convolve(&high_band_filter, &samples[..]);
        let mid_band_filter = bandpass_filter(low_mid_crossover, high_mid_crossover, 0.1);
        let mids = convolve(&mid_band_filter, &samples[..]);
        let zipped = highs
            .into_iter()
            .zip(lows.into_iter())
            .zip(mids.into_iter())
            .take(samples.len());
        let preview_samples = zipped
            .map(|x| {
                let mids = x.1 as f32;
                let lows = x.0 .0 as f32;
                let highs = x.0 .1 as f32;
                let preview_sample = PreviewSample { lows, mids, highs };
                // println!("{:#?}", preview_sample);
                preview_sample
            })
            .collect_vec();
        assert![preview_samples.len() == samples.len()];
        preview_samples
    }
    /// downsample a given buffer of interleaved samples to a summed preview version
    pub fn downsample_to_preview(
        samples: &[f32],
        num_channels: usize,
        target_size: usize,
    ) -> Vec<f32> {
        let chunk_size = samples.len() / target_size;
        let preview_samples = samples
            // sum the channels into on sample
            // .into_iter()
            // .chunks(num_channels)
            // .into_iter()
            // .map(|n_channels_chunk| {
            //     (n_channels_chunk.into_iter().sum::<f32>() / num_channels as f32)
            // })
            // downsample to preview
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
                let mean = sum / num as f32;
                // assert!(mean > 0.0);
                mean
            })
            .take(target_size)
            .collect();
        preview_samples
    }
}
