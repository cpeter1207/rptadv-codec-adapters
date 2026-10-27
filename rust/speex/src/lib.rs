//! Raw Speex NB/WB provider using the released libspeex ABI.
//! Inspection reads framing only; it neither decodes twice nor changes history.
#![deny(unsafe_op_in_unsafe_fn)]

mod framing;
use rptadv_codec_contract::{Config, Descriptor, Mode, Profile, Status};
use rptadv_codec_provider::Backend;
use std::{
    ffi::{c_char, c_int, c_void},
    mem::{MaybeUninit, size_of},
    ptr::NonNull,
};

/// Public SpeexBits layout from Speex 1.2.1 speex_bits.h (BSD-3-Clause).
#[repr(C)]
struct Bits {
    chars: *mut c_char,
    count: c_int,
    byte_cursor: c_int,
    bit_cursor: c_int,
    owner: c_int,
    overflow: c_int,
    buffer_size: c_int,
    reserved1: c_int,
    reserved2: *mut c_void,
}
#[link(name = "speex")]
unsafe extern "C" {
    fn speex_lib_get_mode(mode: c_int) -> *const c_void;
    fn speex_encoder_init(mode: *const c_void) -> *mut c_void;
    fn speex_decoder_init(mode: *const c_void) -> *mut c_void;
    fn speex_encoder_destroy(state: *mut c_void);
    fn speex_decoder_destroy(state: *mut c_void);
    fn speex_encoder_ctl(state: *mut c_void, request: c_int, value: *mut c_void) -> c_int;
    fn speex_decoder_ctl(state: *mut c_void, request: c_int, value: *mut c_void) -> c_int;
    fn speex_encode_int(state: *mut c_void, pcm: *mut i16, bits: *mut Bits) -> c_int;
    fn speex_decode_int(state: *mut c_void, bits: *mut Bits, pcm: *mut i16) -> c_int;
    fn speex_bits_init(bits: *mut Bits);
    fn speex_bits_destroy(bits: *mut Bits);
    fn speex_bits_reset(bits: *mut Bits);
    fn speex_bits_read_from(bits: *mut Bits, packet: *const c_char, bytes: c_int);
    fn speex_bits_pack(bits: *mut Bits, value: c_int, count: c_int);
    fn speex_bits_write(bits: *mut Bits, out: *mut c_char, maximum: c_int) -> c_int;
}
static MODES: [Mode; 2] = [
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 0,
        bitrate_bps: 8000,
        frame_samples: 160,
        frame_bytes: 0,
    },
    Mode {
        struct_size: size_of::<Mode>() as u32,
        id: 0,
        bitrate_bps: 9800,
        frame_samples: 320,
        frame_bytes: 0,
    },
];

struct Speex {
    state: NonNull<c_void>,
    bits: Bits,
    encoder: bool,
    wide: bool,
}
impl Drop for Speex {
    fn drop(&mut self) {
        unsafe {
            speex_bits_destroy(&mut self.bits);
            if self.encoder {
                speex_encoder_destroy(self.state.as_ptr());
            } else {
                speex_decoder_destroy(self.state.as_ptr());
            }
        }
    }
}
impl Backend for Speex {
    const COUNT: usize = 2;
    fn profile(index: usize) -> Option<Profile> {
        if index >= Self::COUNT {
            return None;
        }
        Some(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: if index == 0 { 10 } else { 15 },
            sample_rate_hz: if index == 0 { 8000 } else { 16000 },
            channels: 1,
            directions: 3,
            lookahead_frames: if index == 0 { 40 } else { 143 },
            maximum_frame_samples: MODES[index].frame_samples,
            maximum_frame_bytes: if index == 0 { 63 } else { 107 },
            mode_count: 1,
            modes: &MODES[index],
        })
    }
    fn open(config: &Config, encoder: bool) -> Result<Self, Status> {
        if !matches!(config.profile_id, 10 | 15) || config.mode_id != 0 {
            return Err(Status::Unsupported);
        }
        if config.max_packet_bytes > c_int::MAX as usize {
            return Err(Status::Malformed);
        }
        let wide = config.profile_id == 15;
        let mode = unsafe { speex_lib_get_mode(c_int::from(wide)) };
        let state = unsafe {
            if encoder {
                speex_encoder_init(mode)
            } else {
                speex_decoder_init(mode)
            }
        };
        let state = NonNull::new(state).ok_or(Status::Failed)?;
        let mut bits = MaybeUninit::<Bits>::uninit();
        unsafe {
            speex_bits_init(bits.as_mut_ptr());
        }
        let context = Self {
            state,
            bits: unsafe { bits.assume_init() },
            encoder,
            wide,
        };
        if encoder {
            for (request, mut value) in [(4, 3 as c_int), (12, 0), (30, 0), (34, 0)] {
                if unsafe {
                    speex_encoder_ctl(state.as_ptr(), request, (&mut value as *mut c_int).cast())
                } != 0
                {
                    return Err(Status::Failed);
                }
            }
        }
        Ok(context)
    }
    fn unit(&self) -> (usize, usize) {
        if self.wide { (320, 107) } else { (160, 63) }
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        framing::frames(packet)?
            .checked_mul(self.unit().0 as u32)
            .ok_or(Status::Malformed)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        unsafe {
            speex_bits_reset(&mut self.bits);
        }
        for frame in pcm.chunks_exact(self.unit().0) {
            let mut linear = [0i16; 320];
            for (target, sample) in linear.iter_mut().zip(frame) {
                *target = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
            }
            if unsafe { speex_encode_int(self.state.as_ptr(), linear.as_mut_ptr(), &mut self.bits) }
                < 0
            {
                return Err(Status::Failed);
            }
        }
        unsafe {
            speex_bits_pack(&mut self.bits, 15, 5);
        }
        let count = unsafe {
            speex_bits_write(&mut self.bits, out.as_mut_ptr().cast(), out.len() as c_int)
        };
        usize::try_from(count).map_err(|_| Status::Failed)
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        unsafe {
            speex_bits_read_from(
                &mut self.bits,
                packet.as_ptr().cast(),
                packet.len() as c_int,
            );
        }
        let unit = self.unit().0;
        let count = out.len();
        for frame in out.chunks_exact_mut(unit) {
            let mut linear = [0i16; 320];
            if unsafe { speex_decode_int(self.state.as_ptr(), &mut self.bits, linear.as_mut_ptr()) }
                != 0
            {
                return Err(Status::Malformed);
            }
            for (sample, target) in linear.iter().zip(frame) {
                *target = f32::from(*sample) / 32768.0;
            }
        }
        Ok(count)
    }
    fn reset(&mut self) -> Result<(), Status> {
        unsafe {
            speex_bits_reset(&mut self.bits);
        }
        let result = unsafe {
            if self.encoder {
                speex_encoder_ctl(self.state.as_ptr(), 26, std::ptr::null_mut())
            } else {
                speex_decoder_ctl(self.state.as_ptr(), 26, std::ptr::null_mut())
            }
        };
        if result != 0 {
            return Err(Status::Failed);
        }
        Ok(())
    }
    fn conceal(&mut self, out: &mut [f32]) -> Result<(), Status> {
        let unit = self.unit().0;
        if out.len() % unit != 0 || out.len() > unit * 6 {
            return Err(Status::Unsupported);
        }
        for frame in out.chunks_exact_mut(unit) {
            let mut linear = [0i16; 320];
            // The released Speex API requests native PLC with a null bitstream.
            if unsafe {
                speex_decode_int(
                    self.state.as_ptr(),
                    std::ptr::null_mut(),
                    linear.as_mut_ptr(),
                )
            } != 0
            {
                return Err(Status::Failed);
            }
            for (target, sample) in frame.iter_mut().zip(linear) {
                *target = f32::from(sample) / 32768.0;
            }
        }
        Ok(())
    }
}

static DESCRIPTOR: Descriptor = rptadv_codec_provider::descriptor::<Speex>();
/// Return the immutable Speex NB/WB provider table.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
