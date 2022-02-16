use std::hash::Hash;
use std::path::Path;
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use symphonia::core::codecs::CodecParameters;

use crate::core::analyzer::{PreviewSample, PREVIEW_SAMPLES_PER_PACKET};

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
    codec_params: CodecParameters,
    /// downsampled version of decoded frames for preview
    preview_buffer: RwLock<Vec<PreviewSample>>,
    /// number of samples per packet
    /// This is used to compute the progress of the analysis
    estimated_samples_per_packet: RwLock<Option<usize>>,
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
            estimated_samples_per_packet: RwLock::new(None),
        }
    }

    /// Sets the estimated samples per packet for the track.
    /// This is needed for the progress computation, when the codec parameters don't contain this
    /// information.
    ///
    /// ATTENTION: This is a weird hack, if there is a better solution, use that!
    pub fn set_estimated_samples_per_packet(&self, estimated_samples_per_packet: usize) {
        let current_estimated_samples_per_packet =
            self.estimated_samples_per_packet.read().unwrap().clone();
        if let None = current_estimated_samples_per_packet {
            *self.estimated_samples_per_packet.write().unwrap() =
                // devide the given estimated_samples_per_packet by number of challenge, since the
                // estimated_samples_per_packet is in interlaeved format
                Some(estimated_samples_per_packet / self.codec_params.channels.unwrap().count());
        }
    }

    /// append preview samples to preview buffer
    pub fn append_preview_samples(&self, preview_samples: &mut Vec<PreviewSample>) {
        // Hack: this sets the frames per packet
        // if self.avg_frames_per_packet == None {
        //     self.avg_frames_per_packet = Some((samples.len() / 2) as u64);
        // }
        // since the samples in the packets are interlaeved (2 channels), we have to adjust the
        // chunk size
        self.preview_buffer.write().unwrap().append(preview_samples);
    }

    /// returns the analysis progress for this track.
    /// The result is a number between 0 and 100 (%).
    pub fn progress(&self) -> Option<u8> {
        let mut res = None;
        let estimated_samples_per_packet =
            self.estimated_samples_per_packet.read().unwrap().clone();
        // if codec params contains max_frames_per_packet use that
        // else if estimated_samples_per_packet is set use that
        // else default to 0
        let max_frames_per_packet = self
            .codec_params
            .max_frames_per_packet
            .or(estimated_samples_per_packet.map(|x| x as u64));
        // when max_frames_per_packet and number of total frames in the track are known we can
        // compute the progress
        if let (Some(max_frames_per_packet), Some(n_frames)) =
            (max_frames_per_packet, self.codec_params.n_frames)
        {
            let n_analyzed_packets =
                self.preview_buffer.read().unwrap().len() / PREVIEW_SAMPLES_PER_PACKET;
            let n_analyzed_frames = n_analyzed_packets as u64 * max_frames_per_packet;
            // std::thread::sleep(Duration::from_millis(100));
            // println!("{}/{}", n_analyzed_packets, n_frames);
            res = Some((n_analyzed_frames as f64 / n_frames as f64 * 100.0).ceil() as u8);
        }
        res
    }

    /// returns the preview samples for a given player position and target screen size
    /// the playhead position shifts the player position by [-target_size/2, target_size/2] relative in the buffer
    pub fn preview(
        &self,
        target_size: usize,
        player_position: usize,
        playhead_position: usize,
    ) -> Vec<PreviewSample> {
        let mut preview_buffer = self.preview_buffer.read().unwrap().to_owned();
        // println!("{}", preview_buffer.len());
        let player_pos = player_position * PREVIEW_SAMPLES_PER_PACKET;
        // check if enough sampes exist for target resolution
        let diff = player_pos as isize - (target_size as isize / 2);
        if diff >= 0 {
            // if yes return buffer content
            let l = player_pos as f32 - (target_size as f32 / 2.0);
            let r = player_pos as f32 + (target_size as f32 / 2.0);
            preview_buffer[l as usize..r as usize].to_owned()
        } else {
            let diff = diff.abs() as usize;
            let mut padding = vec![0.0 as f32; diff];
            padding.extend_from_slice(&preview_buffer[0..target_size - diff]);
            padding.to_owned()
        }
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
