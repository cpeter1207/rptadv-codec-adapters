//! Raw GSM FR, dynamically backed by libgsm. WAV49 packing is not accepted.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{
    ffi::{c_int, c_void},
    mem::size_of,
    ptr::NonNull,
};

#[link(name = "gsm")]
unsafe extern "C" {
    fn gsm_create() -> *mut c_void;
    fn gsm_destroy(state: *mut c_void);
    fn gsm_encode(state: *mut c_void, input: *mut i16, output: *mut u8);
    fn gsm_decode(state: *mut c_void, input: *mut u8, output: *mut i16) -> c_int;
}

static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 13200,
    frame_samples: 160,
    frame_bytes: 33,
};

struct Gsm {
    state: NonNull<c_void>,
}
impl Gsm {
    fn new() -> Result<Self, Status> {
        NonNull::new(unsafe { gsm_create() })
            .map(|state| Self { state })
            .ok_or(Status::Failed)
    }
}
impl Drop for Gsm {
    fn drop(&mut self) {
        unsafe {
            gsm_destroy(self.state.as_ptr());
        }
    }
}

impl Backend for Gsm {
    const COUNT: usize = 1;
    fn profile(index: usize) -> Option<Profile> {
        (index == 0).then_some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: 3,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: 160,
            maximum_frame_bytes: 33,
            mode_count: 1,
            modes: &MODE,
        })
    }
    fn open(config: &Config, _encoder: bool) -> Result<Self, Status> {
        if config.profile_id != 3 || config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        Self::new()
    }
    fn unit(&self) -> (usize, usize) {
        (160, 33)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        if packet.len() % 33 != 0 || packet.chunks_exact(33).any(|frame| frame[0] >> 4 != 0xD) {
            return Err(Status::Malformed);
        }
        u32::try_from(packet.len() / 33 * 160).map_err(|_| Status::Malformed)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (input, output) in pcm.chunks_exact(160).zip(out.chunks_exact_mut(33)) {
            let mut linear = [0i16; 160];
            for (target, sample) in linear.iter_mut().zip(input) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            unsafe {
                gsm_encode(
                    self.state.as_ptr(),
                    linear.as_mut_ptr(),
                    output.as_mut_ptr(),
                );
            }
        }
        Ok(pcm.len() / 160 * 33)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        for (input, output) in packet.chunks_exact(33).zip(out.chunks_exact_mut(160)) {
            let mut encoded = [0u8; 33];
            encoded.copy_from_slice(input);
            let mut linear = [0i16; 160];
            if unsafe {
                gsm_decode(
                    self.state.as_ptr(),
                    encoded.as_mut_ptr(),
                    linear.as_mut_ptr(),
                )
            } != 0
            {
                return Err(Status::Malformed);
            }
            for (target, sample) in output.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
        }
        Ok(packet.len() / 33 * 160)
    }
    fn reset(&mut self) -> Result<(), Status> {
        *self = Self::new()?;
        Ok(())
    }
}

static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Gsm>();
/// Return the immutable raw-GSM provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
