//! Independent G.711 mu-law/A-law provider backed by released 0BSD algorithms.
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::mem::size_of;

static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 64_000,
    frame_samples: 1,
    frame_bytes: 1,
};

struct G711 {
    mu: bool,
}

impl Backend for G711 {
    const COUNT: usize = 2;
    fn profile(index: usize) -> Option<Profile> {
        if index >= Self::COUNT {
            return None;
        }
        Some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: index as u32 + 1,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: 1,
            maximum_frame_bytes: 1,
            mode_count: 1,
            modes: &MODE,
        })
    }
    fn open(config: &Config, _encoder: bool) -> Result<Self, Status> {
        if !matches!(config.profile_id, 1 | 2) || config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        Ok(Self {
            mu: config.profile_id == 1,
        })
    }
    fn unit(&self) -> (usize, usize) {
        (1, 1)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        Ok(packet.len() as u32)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (output, &sample) in out.iter_mut().zip(pcm) {
            let linear = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            *output = if self.mu {
                audio_codec_algorithms::encode_ulaw(linear)
            } else {
                audio_codec_algorithms::encode_alaw(linear)
            };
        }
        Ok(pcm.len())
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        for (output, &sample) in out.iter_mut().zip(packet) {
            let linear = if self.mu {
                audio_codec_algorithms::decode_ulaw(sample)
            } else {
                audio_codec_algorithms::decode_alaw(sample)
            };
            *output = f32::from(linear) / 32768.0;
        }
        Ok(packet.len())
    }
    fn reset(&mut self) -> Result<(), Status> {
        Ok(())
    }
}

static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<G711>();

/// Return the immutable complete codec table; retain the DSO through cleanup.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
