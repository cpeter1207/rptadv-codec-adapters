//! Protocol-neutral, C-compatible codec descriptor layouts. No codec implementation.
#![deny(missing_docs)]

use std::ffi::c_void;

/// Exact first-release descriptor ABI.
pub const ABI_VERSION: u32 = 1;
/// Exact NUL-padded capability identity.
pub const CAPABILITY: [u8; 16] = *b"rptadv.codec.v1\0";

/// Result of one bounded provider operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Status {
    /// Operation completed; encode output may contain one packet.
    Complete = 0,
    /// Input was accepted but no complete packet is ready.
    NeedInput = 1,
    /// Output capacity is insufficient; no input/state was consumed.
    OutputFull = 2,
    /// Invalid sized record, input or count.
    Malformed = 3,
    /// This profile, direction or mode is not provided.
    Unsupported = 4,
    /// Backend initialization or processing failed.
    Failed = 5,
}

/// One immutable profile mode; zero encoded bytes denotes a variable-length frame.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Mode {
    /// Record size, including this field.
    pub struct_size: u32,
    /// Provider profile-local mode identifier.
    pub id: u32,
    /// Encoded speech bitrate in bits per second.
    pub bitrate_bps: u32,
    /// PCM frames in one legal coding unit.
    pub frame_samples: u32,
    /// Bytes in one coding unit, or zero for variable length.
    pub frame_bytes: u32,
}

/// One immutable neutral profile. The provider owns the modes for its DSO lifetime.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Profile {
    /// Caller-supplied complete record size.
    pub struct_size: u32,
    /// Stable neutral ID, not a protocol's wire-format bit.
    pub id: u32,
    /// Actual PCM sample rate.
    pub sample_rate_hz: u32,
    /// Number of PCM channels; initial profiles are mono.
    pub channels: u32,
    /// Supported directions: encode=1, decode=2.
    pub directions: u32,
    /// Backend lookahead in source-rate PCM frames.
    pub lookahead_frames: u32,
    /// Largest coding unit's decoded PCM frames.
    pub maximum_frame_samples: u32,
    /// Largest coding unit's encoded byte count.
    pub maximum_frame_bytes: u32,
    /// Number of immutable mode records.
    pub mode_count: usize,
    /// Borrowed immutable modes; never retained past provider lifetime.
    pub modes: *const Mode,
}

/// Parameters copied while opening one independent direction/context.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Config {
    /// Caller-supplied complete record size.
    pub struct_size: u32,
    /// Selected neutral profile ID.
    pub profile_id: u32,
    /// Selected profile-local mode.
    pub mode_id: u32,
    /// Desired encoder packet duration in source PCM frames.
    pub packet_frames: u32,
    /// Caller-derived maximum accepted encoded packet bytes.
    pub max_packet_bytes: usize,
}

/// Non-consuming inspection result for one complete encoded packet.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct PacketInfo {
    /// Caller-supplied complete record size.
    pub struct_size: u32,
    /// Decoded packet span in source-rate PCM frames.
    pub frames: u32,
    /// Validated profile-local mode.
    pub mode: u32,
    /// Required output capacity on a capacity error.
    pub required_capacity: usize,
}

/// One encoder step: consumed input, encoded bytes and duration are independent.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct EncodeResult {
    /// Caller-supplied complete record size.
    pub struct_size: u32,
    /// Numeric [`Status`].
    pub status: u32,
    /// Input PCM frames consumed by this call only.
    pub consumed_frames: usize,
    /// Encoded bytes produced into the caller's buffer.
    pub encoded_bytes: usize,
    /// First real source frame represented, relative to encoder reset.
    pub source_start_frames: u64,
    /// Real source frames represented, excluding finite tail padding.
    pub source_frames: u32,
    /// Decoded wire-packet frames, including required codec padding.
    pub packet_frames: u32,
    /// Required output bytes if status is OutputFull.
    pub required_capacity: usize,
}

/// Immutable operations published independently by every provider DSO.
///
/// All callbacks run outside audio. Contexts have a single owner; callers retain
/// the DSO and its complete table through context destruction. Pointer/count
/// inputs must reference valid aligned storage for the synchronous call. Input
/// and output buffers must not overlap. Record sizes are checked before access.
#[repr(C)]
pub struct Descriptor {
    /// Complete table size.
    pub struct_size: u32,
    /// Exact [`ABI_VERSION`].
    pub abi_version: u32,
    /// Exact [`CAPABILITY`].
    pub capability: [u8; 16],
    /// Number of neutral profiles.
    pub profile_count: Option<extern "C" fn() -> usize>,
    /// Inspect one immutable profile by index.
    pub profile_at: Option<unsafe extern "C" fn(usize, *mut Profile) -> i32>,
    /// Open an independent encoder; sets output null on failure.
    pub open_encoder: Option<unsafe extern "C" fn(*const Config, *mut *mut c_void) -> i32>,
    /// Open an independent decoder; sets output null on failure.
    pub open_decoder: Option<unsafe extern "C" fn(*const Config, *mut *mut c_void) -> i32>,
    /// Validate a packet and return its source-rate sample span without decoding.
    pub inspect:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut PacketInfo) -> i32>,
    /// Consume PCM and produce at most one packet. Empty flush may drain state.
    pub encode: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const f32,
            usize,
            *mut u8,
            usize,
            u32,
            *mut EncodeResult,
        ) -> i32,
    >,
    /// Decode a complete packet; frames receives required capacity on OutputFull.
    pub decode: Option<
        unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut f32, usize, *mut usize) -> i32,
    >,
    /// Reset one context's codec history and source position.
    pub reset: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    /// Destroy an exclusively owned context; null is harmless.
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
}
