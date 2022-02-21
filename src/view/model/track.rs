use std::hash::Hash;
use std::path::Path;
use std::sync::RwLock;

use itertools::Itertools;
use symphonia::core::codecs::CodecParameters;

use crate::core::{
    analyzer::{Analyzer, PreviewSample, PREVIEW_SAMPLE_RATE},
    player::TimeMarker,
};

//------------------------------------------------------------------//
//                              Track                               //
//------------------------------------------------------------------//

#[derive(Debug)]
pub struct Track {
    /// track meta data
    pub meta: TrackMeta,
    /// the file path
    pub file_path: String,
    /// the file name
    pub file_name: String,
    /// codec parameters
    pub codec_params: CodecParameters,
    /// downsampled version of decoded frames for preview
    preview_buffer: RwLock<Vec<PreviewSample>>,
}

impl Track {
    pub fn new(file_path: String, codec_params: CodecParameters) -> Self {
        let file_name = String::from(Path::new(&file_path).file_name().unwrap().to_str().unwrap());
        Self {
            meta: TrackMeta::default(),
            preview_buffer: RwLock::new(vec![]),
            file_path,
            file_name,
            codec_params,
        }
    }

    /// append preview samples to preview buffer
    pub fn append_preview_samples(&self, preview_samples: &mut Vec<PreviewSample>) {
        // Hack: this sets the frames per packet
        // if self.avg_frames_per_packet == None {
        //     self.avg_frames_per_packet = Some((samples.len() / 2) as u64);
        // }
        self.preview_buffer.write().unwrap().append(preview_samples);
    }

    /// returns the analysis progress for this track.
    /// The result is a number between 0 and 100 (%).
    pub fn progress(&self) -> Option<u8> {
        let mut res = 0.;
        let preview_buffer = self.preview_buffer.read().unwrap();

        if let (Some(n_frames), Some(sample_rate)) =
            (self.codec_params.n_frames, self.codec_params.sample_rate)
        {
            if preview_buffer.len() > 0 {
                res = (preview_buffer.len() * (sample_rate / PREVIEW_SAMPLE_RATE) as usize) as f64
                    / (n_frames as f64)
            }
        }
        Some((res * 100.).ceil() as u8)
    }

    /// returns the preview samples for a given player position and target screen size
    /// the playhead position shifts the player position by [-target_size/2, target_size/2] relative in the buffer
    pub fn live_preview(
        &self,
        target_size: usize,
        target_sample_rate: u32,
        playhead_position: &TimeMarker,
    ) -> Vec<PreviewSample> {
        let conversion_factor = PREVIEW_SAMPLE_RATE as f32 / target_sample_rate as f32;
        let mut unscaled = vec![];
        let preview_buffer = self.preview_buffer.read().unwrap();
        // let buffer_len_in_millis = (preview_buffer.len() / PREVIEW_SAMPLE_RATE as usize) * 1000;
        let mut curr_time_in_seconds = playhead_position.get_time_in_seconds();
        let player_pos = (curr_time_in_seconds * PREVIEW_SAMPLE_RATE as f64) as usize;
        let player_pos = player_pos as f32 / conversion_factor;
        // check if enough sampes exist for target resolution
        let diff = player_pos as isize - (target_size / 2) as isize;
        if diff >= 0 {
            // if yes return buffer content
            let l = (player_pos as f32 - (target_size as f32 / 2.0)) as usize;
            let l = (l as f32 * conversion_factor).ceil() as usize;
            let r = (player_pos as f32 + (target_size as f32 / 2.0)) as usize;
            let r = (r as f32 * conversion_factor).ceil() as usize;
            let r = std::cmp::min(r, preview_buffer.len());
            if l < r {
                unscaled = preview_buffer[l..r].to_owned();
            }
        } else {
            let diff = diff.abs() as usize;
            let mut padding: Vec<PreviewSample> =
                vec![0.0 as f32; diff * conversion_factor.floor() as usize]
                    .into_iter()
                    .map(|s| PreviewSample {
                        mids: s,
                        lows: s,
                        highs: s,
                    })
                    .collect();
            if preview_buffer.len() > 0 {
                padding.extend(
                    preview_buffer[0..(target_size - diff) * conversion_factor.floor() as usize]
                        .to_vec(),
                );
            };
            unscaled = padding.to_owned()
        }
        let scaled = unscaled
            .into_iter()
            .chunks(conversion_factor.floor() as usize)
            .into_iter()
            .map(|chunk| {
                let sum: PreviewSample = chunk.into_iter().sum::<PreviewSample>();
                let conversion_rate = conversion_factor.floor();
                let lows = sum.lows / conversion_rate;
                let mids = sum.mids / conversion_rate;
                let highs = sum.highs / conversion_rate;
                PreviewSample { lows, mids, highs }
            })
            .collect();
        scaled
    }

    /// computes a downsampled version of the full track that fits in a buffer of target_size
    pub fn preview(&self, target_size: usize) -> Vec<PreviewSample> {
        let preview_buffer = self.preview_buffer.read().unwrap().clone();
        let conversion_rate =
            PREVIEW_SAMPLE_RATE as f64 / self.codec_params.sample_rate.unwrap() as f64;
        let chunks =
            (self.codec_params.n_frames.unwrap() as f64 * conversion_rate) / target_size as f64;
        // let preview_buffer =
        //     Analyzer::downsample_to_preview(&preview_buffer, num_channles, target_size);
        let preview_buffer = preview_buffer
            .into_iter()
            .chunks(chunks as usize)
            .into_iter()
            .map(|chunk| {
                let sum: PreviewSample = chunk.into_iter().sum::<PreviewSample>();
                let lows = sum.lows / chunks as f32;
                let mids = sum.mids / chunks as f32;
                let highs = sum.highs / chunks as f32;
                PreviewSample { lows, mids, highs }
            })
            .collect();
        return preview_buffer;
    }
}

impl Eq for Track {}

impl PartialOrd for Track {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.file_path.partial_cmp(&other.file_path)
    }
}
impl PartialEq for Track {
    fn eq(&self, other: &Self) -> bool {
        self.file_path == other.file_path
    }
}

impl Ord for Track {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file_path.cmp(&other.file_path)
    }
}

impl Hash for Track {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.file_path.hash(state)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TrackMeta {}
impl Default for TrackMeta {
    fn default() -> Self {
        Self {}
    }
}
