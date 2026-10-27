//! G.729A via the separately packaged, reviewed GPL2 bcg729 1.0.4 backend.
//! The qualified dependency name prevents accidental GPL3-library substitution.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{ffi::c_void, mem::size_of, ptr::NonNull};

#[link(name = "rptadv_bcg729_gpl2")]
unsafe extern "C" {
    fn initBcg729EncoderChannel(vad: u8) -> *mut c_void;
    fn initBcg729DecoderChannel() -> *mut c_void;
    fn closeBcg729EncoderChannel(state: *mut c_void);
    fn closeBcg729DecoderChannel(state: *mut c_void);
    fn bcg729Encoder(state: *mut c_void, pcm: *mut i16, packet: *mut u8, bytes: *mut u8);
    fn bcg729Decoder(
        state: *mut c_void,
        packet: *mut u8,
        bytes: u8,
        erased: u8,
        sid: u8,
        rfc3389: u8,
        pcm: *mut i16,
    );
}
static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 8000,
    frame_samples: 80,
    frame_bytes: 10,
};
struct G729 {
    state: NonNull<c_void>,
    encoder: bool,
}
impl G729 {
    fn new(encoder: bool) -> Result<Self, Status> {
        let pointer = unsafe {
            if encoder {
                initBcg729EncoderChannel(0)
            } else {
                initBcg729DecoderChannel()
            }
        };
        NonNull::new(pointer)
            .map(|state| Self { state, encoder })
            .ok_or(Status::Failed)
    }
}
impl Drop for G729 {
    fn drop(&mut self) {
        unsafe {
            if self.encoder {
                closeBcg729EncoderChannel(self.state.as_ptr());
            } else {
                closeBcg729DecoderChannel(self.state.as_ptr());
            }
        }
    }
}
impl Backend for G729 {
    const COUNT: usize = 1;
    fn profile(index: usize) -> Option<Profile> {
        (index == 0).then_some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: 9,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 40,
            maximum_frame_samples: 80,
            maximum_frame_bytes: 10,
            mode_count: 1,
            modes: &MODE,
        })
    }
    fn open(config: &Config, encoder: bool) -> Result<Self, Status> {
        if config.profile_id != 9 || config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        Self::new(encoder)
    }
    fn unit(&self) -> (usize, usize) {
        (80, 10)
    }
    fn drain_frames(&self) -> Result<usize, Status> {
        Ok(40)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        if !matches!(packet.len() % 10, 0 | 2) {
            return Err(Status::Malformed);
        }
        u32::try_from(packet.len().div_ceil(10) * 80).map_err(|_| Status::Malformed)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (frame, encoded) in pcm.chunks_exact(80).zip(out.chunks_exact_mut(10)) {
            let mut linear = [0i16; 80];
            for (target, sample) in linear.iter_mut().zip(frame) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            let mut count = 0;
            unsafe {
                bcg729Encoder(
                    self.state.as_ptr(),
                    linear.as_mut_ptr(),
                    encoded.as_mut_ptr(),
                    &mut count,
                );
            }
            if count != 10 {
                return Err(Status::Failed);
            }
        }
        Ok(pcm.len() / 80 * 10)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        for (frame, pcm) in packet.chunks(10).zip(out.chunks_exact_mut(80)) {
            let mut linear = [0i16; 80];
            let mut encoded = [0u8; 10];
            encoded[..frame.len()].copy_from_slice(frame);
            unsafe {
                bcg729Decoder(
                    self.state.as_ptr(),
                    encoded.as_mut_ptr(),
                    frame.len() as u8,
                    0,
                    u8::from(frame.len() == 2),
                    0,
                    linear.as_mut_ptr(),
                );
            }
            for (target, sample) in pcm.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
        }
        Ok(packet.len().div_ceil(10) * 80)
    }
    fn reset(&mut self) -> Result<(), Status> {
        *self = Self::new(self.encoder)?;
        Ok(())
    }
    fn conceal(&mut self, out: &mut [f32]) -> Result<(), Status> {
        if out.len() % 80 != 0 || out.len() > 960 {
            return Err(Status::Unsupported);
        }
        for frame in out.chunks_exact_mut(80) {
            let mut linear = [0i16; 80];
            // frameErasureFlag=1 makes the released decoder conceal one 10 ms unit.
            unsafe {
                bcg729Decoder(
                    self.state.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    1,
                    0,
                    0,
                    linear.as_mut_ptr(),
                );
            }
            for (target, sample) in frame.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
        }
        Ok(())
    }
}
static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<G729>();
/// Return the immutable G.729A provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}

#[cfg(test)]
mod tests;
