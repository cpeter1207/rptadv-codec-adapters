//! G.726 packing variants, G.722 and LPC10 through released dynamic SpanDSP.
//! PCM is converted only at the backend's S16 boundary. No resampling or PLC.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{
    ffi::{c_int, c_void},
    mem::size_of,
    ptr::{self, NonNull},
};

#[link(name = "spandsp")]
unsafe extern "C" {
    fn g726_init(state: *mut c_void, rate: c_int, coding: c_int, packing: c_int) -> *mut c_void;
    fn g726_free(state: *mut c_void) -> c_int;
    fn g726_encode(state: *mut c_void, out: *mut u8, input: *const i16, frames: c_int) -> c_int;
    fn g726_decode(state: *mut c_void, out: *mut i16, input: *const u8, bytes: c_int) -> c_int;
    fn g722_encode_init(state: *mut c_void, rate: c_int, options: c_int) -> *mut c_void;
    fn g722_encode_free(state: *mut c_void) -> c_int;
    fn g722_encode(state: *mut c_void, out: *mut u8, input: *const i16, frames: c_int) -> c_int;
    fn g722_decode_init(state: *mut c_void, rate: c_int, options: c_int) -> *mut c_void;
    fn g722_decode_free(state: *mut c_void) -> c_int;
    fn g722_decode(state: *mut c_void, out: *mut i16, input: *const u8, bytes: c_int) -> c_int;
    fn lpc10_encode_init(state: *mut c_void, correction: c_int) -> *mut c_void;
    fn lpc10_encode_free(state: *mut c_void) -> c_int;
    fn lpc10_encode(state: *mut c_void, out: *mut u8, input: *const i16, frames: c_int) -> c_int;
    fn lpc10_decode_init(state: *mut c_void, correction: c_int) -> *mut c_void;
    fn lpc10_decode_free(state: *mut c_void) -> c_int;
    fn lpc10_decode(state: *mut c_void, out: *mut i16, input: *const u8, bytes: c_int) -> c_int;
}

static MODES: [Mode; 3] = [
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 0,
        bitrate_bps: 32000,
        frame_samples: 2,
        frame_bytes: 1,
    },
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 0,
        bitrate_bps: 64000,
        frame_samples: 2,
        frame_bytes: 1,
    },
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 0,
        bitrate_bps: 2400,
        frame_samples: 180,
        frame_bytes: 7,
    },
];

struct Span {
    state: NonNull<c_void>,
    profile: u32,
    encoder: bool,
}
impl Span {
    fn new(profile: u32, encoder: bool) -> Result<Self, Status> {
        let state = unsafe {
            match profile {
                5 | 12 => g726_init(ptr::null_mut(), 32000, 0, if profile == 5 { 1 } else { 2 }),
                13 if encoder => g722_encode_init(ptr::null_mut(), 64000, 0),
                13 => g722_decode_init(ptr::null_mut(), 64000, 0),
                8 if encoder => lpc10_encode_init(ptr::null_mut(), 1),
                8 => lpc10_decode_init(ptr::null_mut(), 1),
                _ => return Err(Status::Unsupported),
            }
        };
        NonNull::new(state)
            .map(|state| Self {
                state,
                profile,
                encoder,
            })
            .ok_or(Status::Failed)
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        unsafe {
            match (self.profile, self.encoder) {
                (5 | 12, _) => {
                    g726_free(self.state.as_ptr());
                }
                (13, true) => {
                    g722_encode_free(self.state.as_ptr());
                }
                (13, false) => {
                    g722_decode_free(self.state.as_ptr());
                }
                (_, true) => {
                    lpc10_encode_free(self.state.as_ptr());
                }
                (_, false) => {
                    lpc10_decode_free(self.state.as_ptr());
                }
            }
        }
    }
}
impl Backend for Span {
    const COUNT: usize = 4;
    fn profile(index: usize) -> Option<Profile> {
        let (id, mode) = match index {
            0 => (5, 0),
            1 => (12, 0),
            2 => (13, 1),
            3 => (8, 2),
            _ => return None,
        };
        Some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id,
            sample_rate_hz: if id == 13 { 16000 } else { 8000 },
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: MODES[mode].frame_samples,
            maximum_frame_bytes: MODES[mode].frame_bytes,
            mode_count: 1,
            modes: &MODES[mode],
        })
    }
    fn open(config: &Config, encoder: bool) -> Result<Self, Status> {
        if config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        Self::new(config.profile_id, encoder)
    }
    fn unit(&self) -> (usize, usize) {
        if self.profile == 8 { (180, 7) } else { (2, 1) }
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        let (samples, bytes) = self.unit();
        if packet.len() % bytes != 0 {
            return Err(Status::Malformed);
        }
        u32::try_from(packet.len() / bytes * samples).map_err(|_| Status::Malformed)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        let mut offset = 0;
        for frame in pcm.chunks(360) {
            let mut linear = [0i16; 360];
            for (target, sample) in linear.iter_mut().zip(frame) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            let encoded = unsafe {
                match self.profile {
                    5 | 12 => g726_encode(
                        self.state.as_ptr(),
                        out[offset..].as_mut_ptr(),
                        linear.as_ptr(),
                        frame.len() as c_int,
                    ),
                    13 => g722_encode(
                        self.state.as_ptr(),
                        out[offset..].as_mut_ptr(),
                        linear.as_ptr(),
                        frame.len() as c_int,
                    ),
                    _ => lpc10_encode(
                        self.state.as_ptr(),
                        out[offset..].as_mut_ptr(),
                        linear.as_ptr(),
                        frame.len() as c_int,
                    ),
                }
            };
            offset += usize::try_from(encoded).map_err(|_| Status::Failed)?;
        }
        Ok(offset)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        let chunk = if self.profile == 8 { 14 } else { 180 };
        let mut offset = 0;
        for frame in packet.chunks(chunk) {
            let mut linear = [0i16; 360];
            let decoded = unsafe {
                match self.profile {
                    5 | 12 => g726_decode(
                        self.state.as_ptr(),
                        linear.as_mut_ptr(),
                        frame.as_ptr(),
                        frame.len() as c_int,
                    ),
                    13 => g722_decode(
                        self.state.as_ptr(),
                        linear.as_mut_ptr(),
                        frame.as_ptr(),
                        frame.len() as c_int,
                    ),
                    _ => lpc10_decode(
                        self.state.as_ptr(),
                        linear.as_mut_ptr(),
                        frame.as_ptr(),
                        frame.len() as c_int,
                    ),
                }
            };
            let count = usize::try_from(decoded).map_err(|_| Status::Malformed)?;
            let expected = frame.len() / self.unit().1 * self.unit().0;
            if count != expected {
                return Err(Status::Failed);
            }
            for (&sample, target) in linear[..count].iter().zip(&mut out[offset..offset + count]) {
                *target = f32::from(sample) / 32768.0;
            }
            offset += count;
        }
        Ok(offset)
    }
    fn reset(&mut self) -> Result<(), Status> {
        *self = Self::new(self.profile, self.encoder)?;
        Ok(())
    }
}

static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Span>();
/// Return the immutable SpanDSP profile table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
