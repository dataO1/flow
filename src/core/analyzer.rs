use crate::core::analyzer;
use crate::view::model;
use samplerate::{ConverterType, Samplerate};
use std::{
    iter::Sum,
    ops::Range,
    sync::Arc,
    thread::{spawn, JoinHandle},
};
use synthrs::filter::{bandpass_filter, convolve, cutoff_from_frequency, lowpass_filter};
use yata::methods::SMA;
use yata::prelude::*;

use itertools::Itertools;
use log::warn;

use std::sync::mpsc::Sender;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CodecParameters, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::{MetadataOptions, Tag},
    probe::Hint,
};

//------------------------------------------------------------------//
//                             Analyzer                             //
//------------------------------------------------------------------//
/// Determines the number of samples in the preview buffer per packet of the original source.
/// Should be a multiple of number of channels
pub const PREVIEW_SAMPLE_RATE: u32 = 2205;

/// This is a mono-summed, downsampled version of a number of decoded samples
#[derive(Copy, Clone, Debug)]
pub struct PreviewSample {
    pub lows: f32,
    pub mids: f32,
    pub highs: f32,
}

impl Sum for PreviewSample {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut lows = 0.;
        let mut mids = 0.;
        let mut highs = 0.;
        for s in iter {
            lows += s.lows;
            mids += s.mids;
            highs += s.highs;
        }
        PreviewSample { lows, mids, highs }
    }
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
    /// Codec Parameters
    codec_params: CodecParameters,
    /// FormatReader
    reader: Box<dyn FormatReader>,
    /// Decoder
    decoder: Box<dyn Decoder>,
    /// Local Cache for analyzed samples
    sample_buf: Vec<f32>,
    /// Local Cache for downsampled samples
    preview_buf: Vec<f32>,
    /// a moving average filter over the analyzed data
    low_moving_avg_filter: SMA,
    mids_moving_avg_filter: SMA,
    highs_moving_avg_filter: SMA,
    peak_intersample_filter: PeakIntersampleFilter,
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
                        analyzer.analyze_bpm(150..200);
                        break;
                    }
                }
            }
        })
    }

    fn new(file_path: String, analyzer_event_out: Sender<analyzer::Event>) -> Self {
        let reader_and_tags = Analyzer::get_reader(file_path.clone());
        let mut reader = reader_and_tags.0;
        let tags = reader_and_tags.1;
        let default_track = reader.default_track().unwrap().clone();
        let decoder = Analyzer::get_decoder(&default_track.codec_params).unwrap();
        let track = Arc::new(model::track::Track::new(
            file_path,
            default_track.codec_params.clone(),
        ));
        if let Some(tags) = tags{
            track.meta.write().unwrap().parse_from(tags);
        }
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
            low_moving_avg_filter: SMA::new(10, &0.).unwrap(),
            mids_moving_avg_filter: SMA::new(50, &0.).unwrap(),
            highs_moving_avg_filter: SMA::new(3, &0.).unwrap(),
            peak_intersample_filter: PeakIntersampleFilter::new(),
            codec_params: default_track.codec_params,
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
    fn get_reader(path: String) -> (Box<dyn FormatReader>, Option<Vec<Tag>>) {
        let src = std::fs::File::open(path).expect("failed to open media");
        let mss = MediaSourceStream::new(Box::new(src), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        let mut probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");
        let mut tags = probed
            .metadata
            .get()
            .map(|meta| meta.current().map(|x| x.tags().to_vec()))
            .flatten();
        (probed.format, tags)
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
        // cache decoded frames
        self.sample_buf.extend_from_slice(samples);
        // let mut samples =
        //     Analyzer::downsample_to_fixed_size(&samples, num_channels, PREVIEW_SAMPLE_RATE);
        self.preview_buf.extend_from_slice(samples);
        // when we have at least a second of material, resample and scan it
        if self.preview_buf.len() >= 10 * self.codec_params.sample_rate.unwrap() as usize {
            let sample_rate = self.track.codec_params.sample_rate.unwrap();
            let num_channels = self.track.codec_params.channels.unwrap().count();
            let converter = Samplerate::new(
                ConverterType::SincFastest,
                sample_rate,
                PREVIEW_SAMPLE_RATE,
                num_channels,
            )
            .unwrap();
            // convert cached downsampled buffer to preview samples
            let samples = &self.preview_buf.clone();
            let samples = self.sum_to_mono(&samples);
            // println!("{}", samples.len());
            // let samples = self.smoothing(&self.preview_buf);
            let samples = converter.process_last(&samples).unwrap();
            let mut preview_samples =
                self.samples_2_preview_samples(&samples, PREVIEW_SAMPLE_RATE as usize);
            self.track.append_preview_samples(&mut preview_samples);
            self.preview_buf = vec![];
        }
    }

    fn analyze_bpm(&mut self, bpm_range: Range<usize>) {
        let samples = self
            .sample_buf
            // .to_vec()
            .iter()
            .map(|s| *s as f64)
            .collect_vec();
        let sample_rate = self.track.codec_params.sample_rate.unwrap();
        let low_crossover = cutoff_from_frequency(200., sample_rate as usize);
        let high_crossover = cutoff_from_frequency(400., sample_rate as usize);
        let low_band_filter = bandpass_filter(low_crossover, high_crossover, 0.01);
        // let samples = convolve(&low_band_filter, &samples);
        // let samples: Vec<f32> = samples.iter().map(|s| *s as f32).collect();
        // analyze bpm
        let buf_s = 2 << 14;
        let hop_s = 256;
        let tempo = std::panic::catch_unwind(|| {
            aubio::Tempo::new(aubio::OnsetMode::Phase, buf_s, hop_s, sample_rate).unwrap()
        });
        match tempo {
            Ok(mut tempo) => {
                for chunk in samples.into_iter().chunks(buf_s).into_iter() {
                    let chunk: Vec<f64> = chunk.collect();
                    // let chunk = convolve(&low_band_filter, &chunk);
                    let chunk = chunk.iter().map(|s| *s as f32).collect_vec();
                    match tempo.do_result(chunk) {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                }
                let t = tempo.get_bpm().floor() as usize;
                // for _ in (0..5) {
                //     if !bpm_range.contains(&t) {
                //         self.analyze_bpm(bpm_range.clone(), hop_s << 2);
                //     };
                // }
                self.track.change_bpm(t as u32);
                // println!("{}", t);
            }
            Err(err) => {
                println!("{:#?}", err);
            }
        };
    }

    fn sum_to_mono(&mut self, samples: &[f32]) -> Vec<f32> {
        let num_channels = self.track.codec_params.channels.unwrap().count();
        samples
            .iter()
            .chunks(num_channels)
            .into_iter()
            .map(|chunk| chunk.into_iter().sum::<f32>() / num_channels as f32)
            .collect()
    }

    fn avg_smoothing_low(&mut self, samples: &[f32]) -> Vec<f32> {
        samples
            .iter()
            .map(move |s| {
                let avg = self.low_moving_avg_filter.next(&(*s as f64));
                avg as f32
            })
            .collect()
    }

    fn avg_smoothing_mid(&mut self, samples: &[f32]) -> Vec<f32> {
        samples
            .iter()
            .map(|s| {
                let avg = self.mids_moving_avg_filter.next(&(*s as f64));
                avg as f32
            })
            .collect()
    }

    fn avg_smoothing_high(&mut self, samples: &[f32]) -> Vec<f32> {
        samples
            .iter()
            .map(|s| {
                let avg = self.highs_moving_avg_filter.next(&(*s as f64));
                avg as f32
            })
            .collect()
    }

    /// convert a buffer of samples into a buffer of preview samples of same lenght
    fn samples_2_preview_samples(
        &mut self,
        samples: &Vec<f32>,
        sample_rate: usize,
    ) -> Vec<PreviewSample> {
        // there are now 441 samples per second
        let samples = samples.iter().map(|s| *s as f64).collect_vec();
        // let sample_rate = 44100 / 2;
        // let low_low_crossover = cutoff_from_frequency(20., sample_rate * 4);
        let high_low_crossover = cutoff_from_frequency(65., sample_rate);
        let low_mid_crossover = cutoff_from_frequency(100., sample_rate);
        let high_mid_crossover = cutoff_from_frequency(400., sample_rate);
        let low_high_crossover = cutoff_from_frequency(800., sample_rate);
        // the maximum high frequency is given by the nyquist freq = sample_rate /2
        let high_high_crossover =
            cutoff_from_frequency(PREVIEW_SAMPLE_RATE as f64 / 2., sample_rate);
        let low_band_filter = lowpass_filter(high_low_crossover, 0.01);
        let lows = convolve(&low_band_filter, &samples);
        let lows = self.peak_intersample_filter.smoothing(&lows);
        let lows = self.avg_smoothing_low(&lows);
        let high_band_filter = bandpass_filter(low_high_crossover, high_high_crossover, 0.01);
        let highs = convolve(&high_band_filter, &samples);
        let highs = self.peak_intersample_filter.smoothing(&highs);
        let highs = self.avg_smoothing_high(&highs);
        let mid_band_filter = bandpass_filter(low_mid_crossover, high_mid_crossover, 0.01);
        let mids = convolve(&mid_band_filter, &samples[..]);
        let mids = self.peak_intersample_filter.smoothing(&mids);
        let mids = self.avg_smoothing_mid(&mids);
        let zipped = highs
            .iter()
            .zip(mids.iter())
            .zip(lows.iter())
            .take(samples.len());
        let preview_samples = zipped
            .map(|x| {
                let lows = *x.1 as f32;
                let highs = *x.0 .0 as f32;
                let mids = *x.0 .1 as f32;
                let preview_sample = PreviewSample { lows, mids, highs };
                // println!("{:#?}", preview_sample);
                preview_sample
            })
            .collect_vec();
        // assert![preview_samples.len() == samples.len()];
        preview_samples
    }
}

//------------------------------------------------------------------//
//                      PeakIntersampleFilter                       //
//------------------------------------------------------------------//

pub struct PeakIntersampleFilter {
    last_peak: f64,
}

impl PeakIntersampleFilter {
    pub fn new() -> Self {
        Self { last_peak: 0.0 }
    }
    pub fn smoothing(&mut self, samples: &[f64]) -> Vec<f32> {
        let mut peaks = vec![];
        let mut second_last = 0.;
        let mut last = self.last_peak;
        let mut skipped = 0;
        for s in samples {
            if *s > 0. && second_last > 0. && last > 0. {
                //detect peak
                if second_last < last && *s < last {
                    for _ in 0..skipped {
                        peaks.push(last as f32);
                    }
                    skipped = 0;
                }
            };
            skipped += 1;
            second_last = last;
            last = *s;
        }
        let diff = samples.len() - peaks.len();
        for _ in 0..diff {
            peaks.push(last as f32);
        }
        self.last_peak = last;
        peaks
    }
}
