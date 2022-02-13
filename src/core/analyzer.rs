use std::sync::{Arc, Mutex};

use log::warn;

use symphonia::core::{
    audio::{SampleBuffer, SignalSpec},
    codecs::{Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};
use tokio::task::JoinHandle;

use super::player::PreviewBuffer;

//------------------------------------------------------------------//
//                             Analyzer                             //
//------------------------------------------------------------------//

pub struct Analyzer {
    /// FormatReader
    reader: Option<Box<dyn FormatReader>>,
    /// Decoder
    decoder: Option<Box<dyn Decoder>>,
    /// Signal Spec
    spec: Option<SignalSpec>,
    /// shared preview buffer
    preview_buffer: Arc<Mutex<PreviewBuffer>>,
}

impl Analyzer {
    pub fn spawn(file_path: String, preview_buffer: Arc<Mutex<PreviewBuffer>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut analyzer = Analyzer::new(preview_buffer);
            // messages
            analyzer.init_reader(file_path);
            analyzer.init_decoder();
            loop {
                match analyzer.decode() {
                    Ok(sample_buffer) => analyzer.analyze_sample_buffer(sample_buffer),
                    Err(_) => {
                        // Error decoding
                        // Probably done here
                        break;
                    }
                }
            }
        })
    }

    fn new(preview_buffer: Arc<Mutex<PreviewBuffer>>) -> Self {
        Self {
            reader: None,
            decoder: None,
            spec: None,
            preview_buffer,
        }
    }

    // creates a new @FormatReader

    fn decode(&mut self) -> Result<SampleBuffer<f32>, Error> {
        match (&mut self.reader, &mut self.decoder) {
            (Some(reader), Some(decoder)) => {
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
            _ => panic![""],
        }
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

    fn analyze_sample_buffer(&self, sample_buffer: SampleBuffer<f32>) {
        self.preview_buffer.lock().unwrap().push(&sample_buffer);
    }
}
