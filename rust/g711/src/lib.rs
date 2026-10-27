//! Independent G.711 mu-law/A-law provider backed by released algorithms under 0BSD.
#![deny(unsafe_op_in_unsafe_fn)]

use rptadv_codec_contract::{Config, Descriptor, EncodeResult, Mode, PacketInfo, Profile, Status};
use std::{
    ffi::c_void,
    mem::{align_of, size_of},
    ptr, slice,
};

static MODE: Mode = Mode {
    struct_size: size_of::<Mode>() as u32,
    id: 0,
    bitrate_bps: 64_000,
    frame_samples: 1,
    frame_bytes: 1,
};

/// Exclusive directional state; no context or encoder history is shared.
struct Context {
    profile: u32,
    maximum: usize,
    encoder: Option<Encoder>,
}

struct Encoder {
    packet: usize,
    pending: Vec<f32>,
    position: u64,
}

extern "C" fn profile_count() -> usize {
    2
}

unsafe extern "C" fn profile_at(index: usize, out: *mut Profile) -> i32 {
    if index >= 2 {
        return Status::Unsupported as i32;
    }
    if !unsafe { sized(out) } {
        return Status::Malformed as i32;
    }
    unsafe {
        out.write(Profile {
            struct_size: size_of::<Profile>() as u32,
            id: index as u32 + 1,
            sample_rate_hz: 8000,
            channels: 1,
            directions: 3,
            lookahead_frames: 0,
            maximum_frame_samples: 1,
            maximum_frame_bytes: 1,
            mode_count: 1,
            modes: &MODE,
        });
    }
    Status::Complete as i32
}

unsafe fn open(config: *const Config, out: *mut *mut c_void, encoder: bool) -> i32 {
    if !valid(out, 1) {
        return Status::Malformed as i32;
    }
    unsafe {
        out.write(ptr::null_mut());
    }
    if !unsafe { sized(config) } {
        return Status::Malformed as i32;
    }
    let config = unsafe { &*config };
    if !matches!(config.profile_id, 1 | 2) || config.mode_id != 0 {
        return Status::Unsupported as i32;
    }
    let packet = config.packet_frames as usize;
    if packet == 0
        || packet > config.max_packet_bytes
        || config.max_packet_bytes > u32::MAX as usize
    {
        return Status::Malformed as i32;
    }
    let encoder = if encoder {
        let mut pending = Vec::new();
        if pending.try_reserve_exact(packet).is_err() {
            return Status::Failed as i32;
        }
        Some(Encoder {
            packet,
            pending,
            position: 0,
        })
    } else {
        None
    };
    let context = Box::new(Context {
        profile: config.profile_id,
        maximum: config.max_packet_bytes,
        encoder,
    });
    unsafe {
        out.write(Box::into_raw(context).cast());
    }
    Status::Complete as i32
}

unsafe extern "C" fn open_encoder(config: *const Config, out: *mut *mut c_void) -> i32 {
    unsafe { open(config, out, true) }
}
unsafe extern "C" fn open_decoder(config: *const Config, out: *mut *mut c_void) -> i32 {
    unsafe { open(config, out, false) }
}

unsafe extern "C" fn inspect(
    context: *mut c_void,
    packet: *const u8,
    bytes: usize,
    out: *mut PacketInfo,
) -> i32 {
    if !valid(context.cast::<Context>(), 1) || !valid(packet, bytes) || !unsafe { sized(out) } {
        return Status::Malformed as i32;
    }
    let context = unsafe { &*context.cast::<Context>() };
    if context.encoder.is_some() || bytes > context.maximum {
        return Status::Malformed as i32;
    }
    unsafe {
        out.write(PacketInfo {
            struct_size: size_of::<PacketInfo>() as u32,
            frames: bytes as u32,
            mode: 0,
            required_capacity: 0,
        });
    }
    Status::Complete as i32
}

unsafe extern "C" fn encode(
    context: *mut c_void,
    pcm: *const f32,
    frames: usize,
    out: *mut u8,
    capacity: usize,
    flush: u32,
    result: *mut EncodeResult,
) -> i32 {
    if !unsafe { sized(result) } {
        return Status::Malformed as i32;
    }
    let mut value = EncodeResult {
        struct_size: size_of::<EncodeResult>() as u32,
        status: Status::Malformed as u32,
        consumed_frames: 0,
        encoded_bytes: 0,
        source_start_frames: 0,
        source_frames: 0,
        packet_frames: 0,
        required_capacity: 0,
    };
    let status = if !valid(context.cast::<Context>(), 1)
        || !valid(pcm, frames)
        || !valid(out, capacity)
        || flush > 1
    {
        Status::Malformed
    } else {
        let context = unsafe { &mut *context.cast::<Context>() };
        let input = if frames == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(pcm, frames) }
        };
        let output = if capacity == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(out, capacity) }
        };
        encode_block(context, input, output, flush != 0, &mut value)
    };
    value.status = status as u32;
    unsafe {
        result.write(value);
    }
    status as i32
}

fn encode_block(
    context: &mut Context,
    pcm: &[f32],
    out: &mut [u8],
    flush: bool,
    result: &mut EncodeResult,
) -> Status {
    let Some(state) = &mut context.encoder else {
        return Status::Unsupported;
    };
    let count = pcm.len().min(state.packet - state.pending.len());
    let total = state.pending.len() + count;
    let ready = total == state.packet || (flush && total != 0);
    if ready && out.len() < total {
        result.required_capacity = total;
        return Status::OutputFull;
    }
    let Some(position) = state.position.checked_add(count as u64) else {
        return Status::Malformed;
    };
    state.pending.extend_from_slice(&pcm[..count]);
    state.position = position;
    result.consumed_frames = count;
    if !ready {
        return if flush {
            Status::Complete
        } else {
            Status::NeedInput
        };
    }
    for (output, &sample) in out.iter_mut().zip(&state.pending) {
        let linear = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
        *output = if context.profile == 1 {
            audio_codec_algorithms::encode_ulaw(linear)
        } else {
            audio_codec_algorithms::encode_alaw(linear)
        };
    }
    result.encoded_bytes = total;
    result.source_start_frames = position - total as u64;
    result.source_frames = total as u32;
    result.packet_frames = total as u32;
    state.pending.clear();
    Status::Complete
}

unsafe extern "C" fn decode(
    context: *mut c_void,
    packet: *const u8,
    bytes: usize,
    out: *mut f32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    if !valid(context.cast::<Context>(), 1)
        || !valid(packet, bytes)
        || !valid(out, capacity)
        || !valid(frames, 1)
    {
        return Status::Malformed as i32;
    }
    unsafe {
        frames.write(0);
    }
    let context = unsafe { &*context.cast::<Context>() };
    if context.encoder.is_some() || bytes > context.maximum {
        return Status::Malformed as i32;
    }
    unsafe {
        frames.write(bytes);
    }
    if capacity < bytes {
        return Status::OutputFull as i32;
    }
    if bytes != 0 {
        let encoded = unsafe { slice::from_raw_parts(packet, bytes) };
        let decoded = unsafe { slice::from_raw_parts_mut(out, bytes) };
        for (output, &sample) in decoded.iter_mut().zip(encoded) {
            let linear = if context.profile == 1 {
                audio_codec_algorithms::decode_ulaw(sample)
            } else {
                audio_codec_algorithms::decode_alaw(sample)
            };
            *output = f32::from(linear) / 32768.0;
        }
    }
    Status::Complete as i32
}

unsafe extern "C" fn reset(context: *mut c_void) -> i32 {
    if !valid(context.cast::<Context>(), 1) {
        return Status::Malformed as i32;
    }
    if let Some(encoder) = &mut unsafe { &mut *context.cast::<Context>() }.encoder {
        encoder.pending.clear();
        encoder.position = 0;
    }
    Status::Complete as i32
}

unsafe extern "C" fn destroy(context: *mut c_void) {
    if !context.is_null() {
        unsafe {
            drop(Box::from_raw(context.cast::<Context>()));
        }
    }
}

fn valid<T>(pointer: *const T, count: usize) -> bool {
    count == 0
        || (!pointer.is_null()
            && pointer as usize % align_of::<T>() == 0
            && count
                .checked_mul(size_of::<T>())
                .is_some_and(|size| size <= isize::MAX as usize))
}

/// The caller guarantees a readable size prefix; no short record is fully borrowed.
unsafe fn sized<T>(pointer: *const T) -> bool {
    valid(pointer, 1) && unsafe { pointer.cast::<u32>().read() } as usize >= size_of::<T>()
}

static DESCRIPTOR: Descriptor = Descriptor {
    struct_size: size_of::<Descriptor>() as u32,
    abi_version: rptadv_codec_contract::ABI_VERSION,
    capability: rptadv_codec_contract::CAPABILITY,
    profile_count: Some(profile_count),
    profile_at: Some(profile_at),
    open_encoder: Some(open_encoder),
    open_decoder: Some(open_decoder),
    inspect: Some(inspect),
    encode: Some(encode),
    decode: Some(decode),
    reset: Some(reset),
    destroy: Some(destroy),
};

/// Return the immutable complete codec table; retain the DSO through context cleanup.
#[unsafe(no_mangle)]
pub extern "C" fn rptadv_codec_descriptor_v1() -> *const Descriptor {
    &DESCRIPTOR
}
