use aubio::{Notes, Tempo};
use symphonia::core::{audio::SampleBuffer, codecs::DecoderOptions};

use crate::core::reader::Reader;

use super::player::Player;

pub struct Analyzer {}

impl Analyzer {
    // pub fn get_tempo(file_path: &str) -> f32 {
    //     // let mut reader = WavReader::open("./test.wav").unwrap();
    //     let mut reader = Reader::get_reader(file_path);
    //     let track = reader.default_track().unwrap();
    //     let num_samples = track.codec_params.n_frames.unwrap();
    //     let sample_rate = track.codec_params.sample_rate.unwrap();
    //     // let sample_format = track.codec_params.sample_format.unwrap();
    //     let dec_opts: DecoderOptions = DecoderOptions {
    //         verify: true,
    //         ..Default::default()
    //     };
    //     let mut decoder = symphonia::default::get_codecs()
    //         .make(&track.codec_params, &dec_opts)
    //         .unwrap();
    //     let mut samples = vec![0 as f32; num_samples as usize];
    //     let mut sample_buf: Option<SampleBuffer<f32>> = None;
    //     while let Ok(p) = reader.next_packet() {
    //         let audio_buf = decoder.decode(&p).unwrap();
    //         // let num_samples = audio_buf.frames();
    //         // Copy the decoded audio buffer into the sample buffer in an interleaved format.
    //         match &mut sample_buf {
    //             Some(buf) => {
    //                 buf.copy_interleaved_ref(audio_buf);
    //                 let packet_samples = buf.samples();
    //                 // println!("{:#?}", packet_samples);
    //                 samples.extend_from_slice(packet_samples);
    //             }
    //             None => {
    //                 println!("init sample buffer!");
    //                 let spec = *audio_buf.spec();
    //                 // Get the capacity of the decoded buffer. Note: This is capacity, not length!
    //                 let duration = audio_buf.capacity() as u64;
    //                 // Create the f32 sample buffer.
    //                 sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
    //             }
    //         }
    //     }
    //     println!("starting analysis");
    //
    //     let mut notes = Notes::new(1024, 1, sample_rate).unwrap();
    //     let notes = match notes.do_result(&samples) {
    //         Ok(x) => x,
    //         Err(err) => {
    //             vec![]
    //         }
    //     };
    //     println!("{:#?}", notes);
    //     let mut tempo = Tempo::new(aubio::OnsetMode::Hfc, 1024, 1, sample_rate).unwrap();
    //     let tempo = match tempo.do_result(&samples) {
    //         Ok(x) => x,
    //         Err(err) => {
    //             println!("{:#?}", err);
    //             0 as f32
    //         }
    //     };
    //     tempo
    // }
}
