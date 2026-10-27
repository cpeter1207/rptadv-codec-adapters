//! IAX-compatible signed PCM: big-endian 8 kHz and little-endian 16 kHz.
//! Serialization is the only implementation here; no DSP or conversion occurs.
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::mem::size_of;

static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 0,
    frame_samples: 1,
    frame_bytes: 2,
};

struct Pcm {
    little: bool,
}

impl Backend for Pcm {
    const COUNT: usize = 2;
    fn profile(index: usize) -> Option<Profile> {
        let (id, rate) = match index {
            0 => (7, 8000),
            1 => (14, 16000),
            _ => return None,
        };
        Some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id,
            sample_rate_hz: rate,
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: 1,
            maximum_frame_bytes: 2,
            mode_count: 1,
            modes: &MODE,
        })
    }
    fn open(config: &Config, _encoder: bool) -> Result<Self, Status> {
        if config.mode_id != 0 || !matches!(config.profile_id, 7 | 14) {
            return Err(Status::Unsupported);
        }
        Ok(Self {
            little: config.profile_id == 14,
        })
    }
    fn unit(&self) -> (usize, usize) {
        (1, 2)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        if packet.len() % 2 != 0 {
            return Err(Status::Malformed);
        }
        Ok((packet.len() / 2) as u32)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (&sample, bytes) in pcm.iter().zip(out.chunks_exact_mut(2)) {
            let sample = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            bytes.copy_from_slice(&if self.little {
                sample.to_le_bytes()
            } else {
                sample.to_be_bytes()
            });
        }
        Ok(pcm.len() * 2)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        for (bytes, sample) in packet.chunks_exact(2).zip(out) {
            let bytes = [bytes[0], bytes[1]];
            *sample = f32::from(if self.little {
                i16::from_le_bytes(bytes)
            } else {
                i16::from_be_bytes(bytes)
            }) / 32768.0;
        }
        Ok(packet.len() / 2)
    }
    fn reset(&mut self) -> Result<(), Status> {
        Ok(())
    }
}

static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Pcm>();

/// Return this provider's immutable ABI-1 descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
