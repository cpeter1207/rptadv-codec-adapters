//! G.723 6.3 kbps encode and 5.3/6.3 kbps decode through dynamic FFmpeg.
//! The C shim only accesses FFmpeg structures; all codec algorithms stay external.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{
    ffi::{c_int, c_void},
    mem::size_of,
    ptr::NonNull,
};

unsafe extern "C" {
    fn rptadv_g723_open(encoder: c_int) -> *mut c_void;
    fn rptadv_g723_close(state: *mut c_void);
    fn rptadv_g723_encode(state: *mut c_void, pcm: *const i16, out: *mut u8) -> c_int;
    fn rptadv_g723_decode(
        state: *mut c_void,
        packet: *const u8,
        bytes: c_int,
        out: *mut i16,
    ) -> c_int;
}
static MODES: [Mode; 2] = [
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 6300,
        bitrate_bps: 6300,
        frame_samples: 240,
        frame_bytes: 24,
    },
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 5300,
        bitrate_bps: 5300,
        frame_samples: 240,
        frame_bytes: 20,
    },
];

struct G723 {
    state: NonNull<c_void>,
    encoder: bool,
    mode: u32,
}
impl G723 {
    fn new(encoder: bool, mode: u32) -> Result<Self, Status> {
        NonNull::new(unsafe { rptadv_g723_open(c_int::from(encoder)) })
            .map(|state| Self {
                state,
                encoder,
                mode,
            })
            .ok_or(Status::Failed)
    }
}
impl Drop for G723 {
    fn drop(&mut self) {
        unsafe {
            rptadv_g723_close(self.state.as_ptr());
        }
    }
}
fn frame_bytes(header: u8) -> usize {
    [24, 20, 4, 1][usize::from(header & 3)]
}
impl Backend for G723 {
    const COUNT: usize = 1;
    fn profile(index: usize) -> Option<Profile> {
        (index == 0).then_some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: 4,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 60,
            maximum_frame_samples: 240,
            maximum_frame_bytes: 24,
            mode_count: 2,
            modes: MODES.as_ptr(),
        })
    }
    fn open(config: &Config, encoder: bool) -> Result<Self, Status> {
        if config.profile_id != 4
            || !matches!(config.mode_id, 5300 | 6300)
            || (encoder && config.mode_id != 6300)
        {
            return Err(Status::Unsupported);
        }
        Self::new(encoder, config.mode_id)
    }
    fn unit(&self) -> (usize, usize) {
        (240, 24)
    }
    fn packet_mode(&self, packet: &[u8]) -> u32 {
        match packet.first().map(|header| header & 3) {
            Some(0) => 6300,
            Some(1) => 5300,
            _ => self.mode,
        }
    }
    fn inspect(&self, mut packet: &[u8]) -> Result<u32, Status> {
        let mut frames = 0u32;
        while let Some(&header) = packet.first() {
            let count = frame_bytes(header);
            if packet.len() < count {
                return Err(Status::Malformed);
            }
            frames = frames.checked_add(240).ok_or(Status::Malformed)?;
            packet = &packet[count..];
        }
        Ok(frames)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (frame, packet) in pcm.chunks_exact(240).zip(out.chunks_exact_mut(24)) {
            let mut linear = [0i16; 240];
            for (target, sample) in linear.iter_mut().zip(frame) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            if unsafe {
                rptadv_g723_encode(self.state.as_ptr(), linear.as_ptr(), packet.as_mut_ptr())
            } != 24
            {
                return Err(Status::Failed);
            }
        }
        Ok(pcm.len() / 240 * 24)
    }
    fn decode(&mut self, mut packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        let mut frames = 0;
        for pcm in out.chunks_exact_mut(240) {
            let count = frame_bytes(packet[0]);
            let mut linear = [0i16; 240];
            if unsafe {
                rptadv_g723_decode(
                    self.state.as_ptr(),
                    packet.as_ptr(),
                    count as c_int,
                    linear.as_mut_ptr(),
                )
            } != 240
            {
                return Err(Status::Malformed);
            }
            for (target, sample) in pcm.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
            frames += 240;
            packet = &packet[count..];
        }
        Ok(frames)
    }
    fn reset(&mut self) -> Result<(), Status> {
        *self = Self::new(self.encoder, self.mode)?;
        Ok(())
    }
}
static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<G723>();
/// Return the immutable G.723 provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
