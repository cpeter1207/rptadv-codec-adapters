//! Explicit 20/30 ms iLBC contexts backed by released libilbc 2.0.2.
//! No guessed aggregate mode or automatic mid-stream mode substitution.
#![deny(unsafe_op_in_unsafe_fn)]
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{
    ffi::c_void,
    mem::size_of,
    ptr::{self, NonNull},
};

#[link(name = "rptadv_ilbc2")]
unsafe extern "C" {
    fn WebRtcIlbcfix_EncoderCreate(out: *mut *mut c_void) -> i16;
    fn WebRtcIlbcfix_DecoderCreate(out: *mut *mut c_void) -> i16;
    fn WebRtcIlbcfix_EncoderFree(state: *mut c_void) -> i16;
    fn WebRtcIlbcfix_DecoderFree(state: *mut c_void) -> i16;
    fn WebRtcIlbcfix_EncoderInit(state: *mut c_void, mode: i16) -> i16;
    fn WebRtcIlbcfix_DecoderInit(state: *mut c_void, mode: i16) -> i16;
    fn WebRtcIlbcfix_Encode(state: *mut c_void, pcm: *const i16, frames: i16, out: *mut i16)
    -> i16;
    fn WebRtcIlbcfix_Decode(
        state: *mut c_void,
        packet: *const i16,
        bytes: i16,
        out: *mut i16,
        speech: *mut i16,
    ) -> i16;
}
static MODES: [Mode; 2] = [
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 30,
        bitrate_bps: 13333,
        frame_samples: 240,
        frame_bytes: 50,
    },
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 20,
        bitrate_bps: 15200,
        frame_samples: 160,
        frame_bytes: 38,
    },
];
struct Ilbc {
    state: NonNull<c_void>,
    encoder: bool,
    mode: u32,
}
impl Ilbc {
    fn initialize(&mut self) -> Result<(), Status> {
        let result = unsafe {
            if self.encoder {
                WebRtcIlbcfix_EncoderInit(self.state.as_ptr(), self.mode as i16)
            } else {
                WebRtcIlbcfix_DecoderInit(self.state.as_ptr(), self.mode as i16)
            }
        };
        if result != 0 {
            return Err(Status::Failed);
        }
        Ok(())
    }
}
impl Drop for Ilbc {
    fn drop(&mut self) {
        unsafe {
            if self.encoder {
                WebRtcIlbcfix_EncoderFree(self.state.as_ptr());
            } else {
                WebRtcIlbcfix_DecoderFree(self.state.as_ptr());
            }
        }
    }
}
impl Backend for Ilbc {
    const COUNT: usize = 1;
    fn profile(index: usize) -> Option<Profile> {
        (index == 0).then_some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: 11,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: 240,
            maximum_frame_bytes: 50,
            mode_count: 2,
            modes: MODES.as_ptr(),
        })
    }
    fn open(config: &Config, encoder: bool) -> Result<Self, Status> {
        if config.profile_id != 11 || !matches!(config.mode_id, 20 | 30) {
            return Err(Status::Unsupported);
        }
        let mut pointer = ptr::null_mut();
        let result = unsafe {
            if encoder {
                WebRtcIlbcfix_EncoderCreate(&mut pointer)
            } else {
                WebRtcIlbcfix_DecoderCreate(&mut pointer)
            }
        };
        let state = NonNull::new(pointer).ok_or(Status::Failed)?;
        let mut context = Self {
            state,
            encoder,
            mode: config.mode_id,
        };
        if result != 0 {
            return Err(Status::Failed);
        }
        context.initialize()?;
        Ok(context)
    }
    fn unit(&self) -> (usize, usize) {
        if self.mode == 20 {
            (160, 38)
        } else {
            (240, 50)
        }
    }
    fn packet_mode(&self, _packet: &[u8]) -> u32 {
        self.mode
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        let (frames, bytes) = self.unit();
        if packet.len() % bytes != 0 {
            return Err(Status::Malformed);
        }
        // libilbc 2.0.2 DecodeImpl silently conceals invalid startIdx/final-bit
        // values. Preflight every unit before any decoder history is changed;
        // concealment belongs exclusively to the caller's shared PCM ring.
        for unit in packet.chunks_exact(bytes) {
            let valid_start = if self.mode == 20 {
                ((unit[2] >> 2) & 3) != 0
            } else {
                (1..=5).contains(&((unit[5] >> 5) & 7))
            };
            if !valid_start || unit[bytes - 1] & 1 != 0 {
                return Err(Status::Malformed);
            }
        }
        u32::try_from(packet.len() / bytes * frames).map_err(|_| Status::Malformed)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        let (unit, bytes) = self.unit();
        for (frame, packet) in pcm.chunks_exact(unit).zip(out.chunks_exact_mut(bytes)) {
            let mut linear = [0i16; 240];
            for (target, sample) in linear.iter_mut().zip(frame) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            let mut encoded = [0i16; 25];
            if unsafe {
                WebRtcIlbcfix_Encode(
                    self.state.as_ptr(),
                    linear.as_ptr(),
                    unit as i16,
                    encoded.as_mut_ptr(),
                )
            } != bytes as i16
            {
                return Err(Status::Failed);
            }
            for (word, target) in encoded.iter().zip(packet.chunks_exact_mut(2)) {
                target.copy_from_slice(&word.to_ne_bytes());
            }
        }
        Ok(pcm.len() / unit * bytes)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        let (unit, bytes) = self.unit();
        for (frame, pcm) in packet.chunks_exact(bytes).zip(out.chunks_exact_mut(unit)) {
            let mut encoded = [0i16; 25];
            for (word, source) in encoded.iter_mut().zip(frame.chunks_exact(2)) {
                *word = i16::from_ne_bytes([source[0], source[1]]);
            }
            let mut linear = [0i16; 240];
            let mut speech = 0;
            if unsafe {
                WebRtcIlbcfix_Decode(
                    self.state.as_ptr(),
                    encoded.as_ptr(),
                    bytes as i16,
                    linear.as_mut_ptr(),
                    &mut speech,
                )
            } != unit as i16
            {
                return Err(Status::Malformed);
            }
            for (target, sample) in pcm.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
        }
        Ok(packet.len() / bytes * unit)
    }
    fn reset(&mut self) -> Result<(), Status> {
        self.initialize()
    }
}
static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Ilbc>();
/// Return the immutable explicit-mode iLBC provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
