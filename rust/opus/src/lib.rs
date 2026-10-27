//! Mono 48 kHz Opus, dynamically backed by libopus. Encoding uses 20 ms units.
//! Received duration is inspected by libopus; packet bytes are never PCM counts.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{
    ffi::{c_int, c_void},
    mem::size_of,
    ptr::NonNull,
};

#[link(name = "opus")]
unsafe extern "C" {
    fn opus_encoder_create(
        rate: c_int,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut c_void;
    fn opus_decoder_create(rate: c_int, channels: c_int, error: *mut c_int) -> *mut c_void;
    fn opus_encoder_destroy(state: *mut c_void);
    fn opus_decoder_destroy(state: *mut c_void);
    fn opus_encoder_ctl(state: *mut c_void, request: c_int, ...) -> c_int;
    fn opus_decoder_ctl(state: *mut c_void, request: c_int, ...) -> c_int;
    fn opus_encode_float(
        state: *mut c_void,
        pcm: *const f32,
        frames: c_int,
        out: *mut u8,
        maximum: c_int,
    ) -> c_int;
    fn opus_decode_float(
        state: *mut c_void,
        packet: *const u8,
        bytes: c_int,
        out: *mut f32,
        maximum: c_int,
        fec: c_int,
    ) -> c_int;
    fn opus_packet_get_nb_samples(packet: *const u8, bytes: c_int, rate: c_int) -> c_int;
}

static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 0,
    frame_samples: 960,
    frame_bytes: 0,
};

struct Opus {
    state: NonNull<c_void>,
    encoder: bool,
}
impl Drop for Opus {
    fn drop(&mut self) {
        unsafe {
            if self.encoder {
                opus_encoder_destroy(self.state.as_ptr());
            } else {
                opus_decoder_destroy(self.state.as_ptr());
            }
        }
    }
}
impl Backend for Opus {
    const COUNT: usize = 1;
    fn profile(index: usize) -> Option<Profile> {
        (index == 0).then_some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: 16,
            sample_rate_hz: 48000,
            channels: 1,
            directions: 3,
            lookahead_frames: 312,
            maximum_frame_samples: 5760,
            maximum_frame_bytes: 1275,
            mode_count: 1,
            modes: &MODE,
        })
    }
    fn open(config: &Config, encoder: bool) -> Result<Self, Status> {
        if config.profile_id != 16 || config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        if config.packet_frames != 960 || config.max_packet_bytes > c_int::MAX as usize {
            return Err(Status::Malformed);
        }
        let mut error = 0;
        let state = unsafe {
            if encoder {
                opus_encoder_create(48000, 1, 2048, &mut error)
            } else {
                opus_decoder_create(48000, 1, &mut error)
            }
        };
        let state = NonNull::new(state).ok_or(Status::Failed)?;
        let context = Self { state, encoder };
        if error != 0 {
            return Err(Status::Failed);
        }
        // Explicitly disable DTX; local transmission/key state is not a codec side effect.
        if encoder && unsafe { opus_encoder_ctl(state.as_ptr(), 4016, 0 as c_int) } != 0 {
            return Err(Status::Failed);
        }
        Ok(context)
    }
    fn unit(&self) -> (usize, usize) {
        (960, 1275)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        let bytes = c_int::try_from(packet.len()).map_err(|_| Status::Malformed)?;
        let frames = unsafe { opus_packet_get_nb_samples(packet.as_ptr(), bytes, 48000) };
        if !(1..=5760).contains(&frames) {
            return Err(Status::Malformed);
        }
        Ok(frames as u32)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        let encoded = unsafe {
            opus_encode_float(
                self.state.as_ptr(),
                pcm.as_ptr(),
                960,
                out.as_mut_ptr(),
                1275,
            )
        };
        usize::try_from(encoded).map_err(|_| Status::Failed)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        let frames = unsafe {
            opus_decode_float(
                self.state.as_ptr(),
                packet.as_ptr(),
                packet.len() as c_int,
                out.as_mut_ptr(),
                out.len() as c_int,
                0,
            )
        };
        usize::try_from(frames).map_err(|_| Status::Malformed)
    }
    fn conceal(&mut self, out: &mut [f32]) -> Result<(), Status> {
        // Opus PLC uses a null packet and a duration in 2.5 ms units, <=120 ms.
        if out.len() % 120 != 0 || out.len() > 5760 {
            return Err(Status::Unsupported);
        }
        let frames = unsafe {
            opus_decode_float(
                self.state.as_ptr(),
                std::ptr::null(),
                0,
                out.as_mut_ptr(),
                out.len() as c_int,
                0,
            )
        };
        if frames != out.len() as c_int {
            return Err(Status::Failed);
        }
        Ok(())
    }
    fn reset(&mut self) -> Result<(), Status> {
        let result = unsafe {
            if self.encoder {
                opus_encoder_ctl(self.state.as_ptr(), 4028)
            } else {
                opus_decoder_ctl(self.state.as_ptr(), 4028)
            }
        };
        if result != 0 {
            return Err(Status::Failed);
        }
        Ok(())
    }
}

static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Opus>();
/// Return the immutable Opus provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
