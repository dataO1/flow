use aubio::{
    vec::{FVec, FVecMut},
    Tempo,
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
};

use super::player::Player;

pub struct Analyzer {}

impl Analyzer {
    pub fn get_tempo(file_path: &str) -> f32 {
        // let mut reader = WavReader::open("./test.wav").unwrap();
        let mut reader = Player::new_reader("music/bass_symptom.mp3");
        let track = reader.default_track().unwrap();
        let sample_rate = track.codec_params.sample_rate.unwrap();
        // let sample_format = track.codec_params.sample_format.unwrap();
        let mut tempo = Tempo::new(aubio::OnsetMode::Hfc, 512, 256, sample_rate).unwrap();
        let num_samples = track.codec_params.n_frames.unwrap() as usize;
        let dec_opts: DecoderOptions = DecoderOptions {
            verify: true,
            ..Default::default()
        };
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &dec_opts)
            .unwrap();
        let mut samples = vec![0 as f32; 10000000000];
        let mut c = 0;
        let mut sample_buf = None;
        while let Ok(p) = reader.next_packet() {
            let audio_buf = decoder.decode(&p).unwrap();
            let num_samples = audio_buf.frames();
            if sample_buf.is_none() {
                let spec = *audio_buf.spec();
                // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                let duration = audio_buf.capacity() as u64;
                // Create the f32 sample buffer.
                sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
            };
            // Copy the decoded audio buffer into the sample buffer in an interleaved format.
            if let Some(buf) = &mut sample_buf {
                buf.copy_interleaved_ref(audio_buf);
                buf.samples();
                for s in buf.samples() {
                    samples[c] = s.clone();
                    c += 1;
                }
            }
        }
        let tempo = tempo.do_result(samples).unwrap();
        tempo
    }
}
