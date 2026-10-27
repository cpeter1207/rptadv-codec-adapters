//! Exact released Asterisk Dialogic ADPCM primitive through its host-free DSO.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{ffi::c_void, mem::size_of, ptr::NonNull};
#[link(name = "rptadv_dialogic")]
unsafe extern "C" {
    fn rptadv_dialogic_open() -> *mut c_void;
    fn rptadv_dialogic_close(state: *mut c_void);
    fn rptadv_dialogic_reset(state: *mut c_void);
    fn rptadv_dialogic_encode(state: *mut c_void, pcm: *const i16, out: *mut u8, frames: usize);
    fn rptadv_dialogic_decode(state: *mut c_void, packet: *const u8, out: *mut i16, bytes: usize);
}
static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 32000,
    frame_samples: 2,
    frame_bytes: 1,
};
struct Dialogic {
    state: NonNull<c_void>,
}
impl Drop for Dialogic {
    fn drop(&mut self) {
        unsafe {
            rptadv_dialogic_close(self.state.as_ptr());
        }
    }
}
impl Backend for Dialogic {
    const COUNT: usize = 1;
    fn profile(index: usize) -> Option<Profile> {
        (index == 0).then_some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: 6,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: 2,
            maximum_frame_bytes: 1,
            mode_count: 1,
            modes: &MODE,
        })
    }
    fn open(config: &Config, _encoder: bool) -> Result<Self, Status> {
        if config.profile_id != 6 || config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        NonNull::new(unsafe { rptadv_dialogic_open() })
            .map(|state| Self { state })
            .ok_or(Status::Failed)
    }
    fn unit(&self) -> (usize, usize) {
        (2, 1)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        u32::try_from(packet.len() * 2).map_err(|_| Status::Malformed)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (frame, packet) in pcm.chunks(320).zip(out.chunks_mut(160)) {
            let mut linear = [0i16; 320];
            for (target, sample) in linear.iter_mut().zip(frame) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            unsafe {
                rptadv_dialogic_encode(
                    self.state.as_ptr(),
                    linear.as_ptr(),
                    packet.as_mut_ptr(),
                    frame.len(),
                );
            }
        }
        Ok(pcm.len() / 2)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        for (frame, pcm) in packet.chunks(160).zip(out.chunks_mut(320)) {
            let mut linear = [0i16; 320];
            unsafe {
                rptadv_dialogic_decode(
                    self.state.as_ptr(),
                    frame.as_ptr(),
                    linear.as_mut_ptr(),
                    frame.len(),
                );
            }
            for (target, sample) in pcm.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
        }
        Ok(packet.len() * 2)
    }
    fn reset(&mut self) -> Result<(), Status> {
        unsafe {
            rptadv_dialogic_reset(self.state.as_ptr());
        }
        Ok(())
    }
}
static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Dialogic>();
/// Return the immutable exact-Dialogic provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
