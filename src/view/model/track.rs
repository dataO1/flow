use std::hash::Hash;
use std::path::Path;
use std::sync::RwLock;

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
        playhead_position: &TimeMarker,
    ) -> Vec<PreviewSample> {
        let preview_buffer = self.preview_buffer.read().unwrap();
        // let buffer_len_in_millis = (preview_buffer.len() / PREVIEW_SAMPLE_RATE as usize) * 1000;
        let mut curr_time_in_seconds = playhead_position.get_time_in_seconds();
        let player_pos = (curr_time_in_seconds * PREVIEW_SAMPLE_RATE as f64) as usize;
        // check if enough sampes exist for target resolution
        let diff = player_pos as isize - (target_size / 2) as isize;
        if diff >= 0 {
            // if yes return buffer content
            let l = (player_pos as f32 - (target_size as f32 / 2.0)) as usize;
            let r = (player_pos as f32 + (target_size as f32 / 2.0)) as usize;
            let r = std::cmp::min(r, preview_buffer.len());
            if l < r {
                preview_buffer[l..r].to_owned()
            } else {
                vec![]
            }
        } else {
            let diff = diff.abs() as usize;
            let mut padding: Vec<PreviewSample> = vec![0.0 as f32; diff]
                .into_iter()
                .map(|s| PreviewSample {
                    mids: s,
                    lows: s,
                    highs: s,
                })
                .collect();
            if preview_buffer.len() > 0 {
                padding.extend(preview_buffer[0..target_size - diff].to_vec());
            };
            padding.to_owned()
        }
    }

    /// computes a downsampled version of the full track that fits in a buffer of target_size
    pub fn preview(&self, target_size: usize) -> Vec<PreviewSample> {
        let preview_buffer = self.preview_buffer.read().unwrap().clone();
        // let preview_buffer =
        //     Analyzer::downsample_to_preview(&preview_buffer, num_channles, target_size);
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
