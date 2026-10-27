//! Shared bounded C-ABI validation and packet assembly, compiled into each provider.
//! Backend DSOs remain independent; no codec, protocol, conversion or PLC lives here.
#![deny(unsafe_op_in_unsafe_fn, missing_docs)]

use rptadv_codec_contract::{Config, Descriptor, EncodeResult, PacketInfo, Profile, Status};
use std::{
    ffi::c_void,
    mem::{align_of, size_of},
    ptr, slice,
};

/// A released backend's exclusively owned directional state. Called off audio.
pub trait Backend: Sized {
    /// Number of immutable neutral profiles in this provider.
    const COUNT: usize;
    /// Return one profile, including its provider-owned immutable mode table.
    fn profile(index: usize) -> Option<Profile>;
    /// Validate the selected profile/mode and open one direction.
    fn open(config: &Config, encoder: bool) -> Result<Self, Status>;
    /// Encoding unit: source frames and maximum encoded bytes.
    fn unit(&self) -> (usize, usize);
    /// Zero-input padding needed to preserve a finite source's delayed tail.
    /// This is a drain budget, not a claim about decoder filter delay.
    fn drain_frames(&self) -> Result<usize, Status> {
        Ok(0)
    }
    /// Profile-local mode reported for a validated packet (zero for single-mode profiles).
    fn packet_mode(&self, _packet: &[u8]) -> u32 {
        0
    }
    /// Inspect a complete nonempty packet without modifying decoder history.
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status>;
    /// Encode complete units. Output is already large enough for the maximum.
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status>;
    /// Decode a whole inspected packet; caller capacity has been checked.
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status>;
    /// Use released decoder PLC, never a new concealment algorithm. Unsupported
    /// spans must leave history/output unchanged so the ring can supply fallback.
    fn conceal(&mut self, _out: &mut [f32]) -> Result<(), Status> {
        Err(Status::Unsupported)
    }
    /// Restore initial backend history without changing the selected mode.
    fn reset(&mut self) -> Result<(), Status>;
}

struct Context<B> {
    backend: B,
    maximum: usize,
    encoder: Option<Encoder>,
}

struct Encoder {
    packet: usize,
    pending: Vec<f32>,
    position: u64,
    wire_position: u64,
    drain: usize,
    finished: bool,
}

/// Produce a complete typed table for a backend without duplicating ABI glue.
pub const fn descriptor<B: Backend>() -> Descriptor {
    Descriptor {
        struct_size: size_of::<Descriptor>() as u32,
        abi_version: rptadv_codec_contract::ABI_VERSION,
        capability: rptadv_codec_contract::CAPABILITY,
        profile_count: Some(count::<B>),
        profile_at: Some(profile::<B>),
        open_encoder: Some(open_encoder::<B>),
        open_decoder: Some(open_decoder::<B>),
        inspect: Some(inspect::<B>),
        encode: Some(encode::<B>),
        decode: Some(decode::<B>),
        reset: Some(reset::<B>),
        destroy: Some(destroy::<B>),
        conceal: Some(conceal::<B>),
    }
}

extern "C" fn count<B: Backend>() -> usize {
    B::COUNT
}

unsafe extern "C" fn profile<B: Backend>(index: usize, out: *mut Profile) -> i32 {
    if !unsafe { sized(out) } {
        return Status::Malformed as i32;
    }
    let Some(value) = B::profile(index) else {
        return Status::Unsupported as i32;
    };
    unsafe {
        out.write(value);
    }
    Status::Complete as i32
}

unsafe fn open<B: Backend>(config: *const Config, out: *mut *mut c_void, encoder: bool) -> i32 {
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
    let prepared = (|| {
        let backend = B::open(config, encoder)?;
        let (unit, bytes) = backend.unit();
        let packet = config.packet_frames as usize;
        if unit == 0
            || bytes == 0
            || packet == 0
            || packet % unit != 0
            || config.max_packet_bytes > u32::MAX as usize
            || packet
                .checked_div(unit)
                .and_then(|n| n.checked_mul(bytes))
                .is_none_or(|n| n > config.max_packet_bytes)
        {
            return Err(Status::Malformed);
        }
        let state = if encoder {
            let mut pending = Vec::new();
            pending
                .try_reserve_exact(packet)
                .map_err(|_| Status::Failed)?;
            Some(Encoder {
                packet,
                pending,
                position: 0,
                wire_position: 0,
                drain: backend.drain_frames()?,
                finished: false,
            })
        } else {
            None
        };
        Ok(Box::new(Context {
            backend,
            maximum: config.max_packet_bytes,
            encoder: state,
        }))
    })();
    match prepared {
        Ok(context) => {
            unsafe {
                out.write(Box::into_raw(context).cast());
            }
            Status::Complete as i32
        }
        Err(status) => status as i32,
    }
}

unsafe extern "C" fn open_encoder<B: Backend>(config: *const Config, out: *mut *mut c_void) -> i32 {
    unsafe { open::<B>(config, out, true) }
}
unsafe extern "C" fn open_decoder<B: Backend>(config: *const Config, out: *mut *mut c_void) -> i32 {
    unsafe { open::<B>(config, out, false) }
}

fn packet_frames<B: Backend>(context: &Context<B>, packet: &[u8]) -> Result<u32, Status> {
    if context.encoder.is_some() || packet.len() > context.maximum {
        return Err(Status::Malformed);
    }
    if packet.is_empty() {
        Ok(0)
    } else {
        context.backend.inspect(packet)
    }
}

unsafe extern "C" fn inspect<B: Backend>(
    context: *mut c_void,
    packet: *const u8,
    bytes: usize,
    out: *mut PacketInfo,
) -> i32 {
    if !valid(context.cast::<Context<B>>(), 1) || !valid(packet, bytes) || !unsafe { sized(out) } {
        return Status::Malformed as i32;
    }
    let context = unsafe { &*context.cast::<Context<B>>() };
    match packet_frames(context, unsafe { input(packet, bytes) }) {
        Ok(frames) => {
            unsafe {
                out.write(PacketInfo {
                    struct_size: size_of::<PacketInfo>() as u32,
                    frames,
                    mode: context.backend.packet_mode(input(packet, bytes)),
                    required_capacity: 0,
                });
            }
            Status::Complete as i32
        }
        Err(status) => status as i32,
    }
}

unsafe extern "C" fn encode<B: Backend>(
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
        status: 0,
        consumed_frames: 0,
        encoded_bytes: 0,
        source_start_frames: 0,
        source_frames: 0,
        packet_frames: 0,
        required_capacity: 0,
    };
    let status = if !valid(context.cast::<Context<B>>(), 1)
        || !valid(pcm, frames)
        || !valid(out, capacity)
        || flush > 1
    {
        Status::Malformed
    } else {
        encode_block(
            unsafe { &mut *context.cast::<Context<B>>() },
            unsafe { input(pcm, frames) },
            unsafe { output(out, capacity) },
            flush != 0,
            &mut value,
        )
    };
    value.status = status as u32;
    unsafe {
        result.write(value);
    }
    status as i32
}

fn encode_block<B: Backend>(
    context: &mut Context<B>,
    pcm: &[f32],
    out: &mut [u8],
    flush: bool,
    result: &mut EncodeResult,
) -> Status {
    let Some(state) = &mut context.encoder else {
        return Status::Unsupported;
    };
    if state.finished && !pcm.is_empty() {
        return Status::Malformed;
    }
    let count = pcm.len().min(state.packet - state.pending.len());
    let real = state.pending.len() + count;
    let Some(position) = state.position.checked_add(count as u64) else {
        return Status::Malformed;
    };
    let Some(end) = position.checked_add(state.drain as u64) else {
        return Status::Malformed;
    };
    let remaining = if position == 0 {
        0
    } else {
        end.saturating_sub(state.wire_position)
    };
    let ready = real == state.packet || (flush && (real != 0 || remaining != 0));
    let (unit, bytes) = context.backend.unit();
    let wire = if real != 0 {
        real.div_ceil(unit) * unit
    } else {
        (remaining.min(state.packet as u64) as usize).div_ceil(unit) * unit
    };
    let required = wire / unit * bytes;
    if ready && out.len() < required {
        result.required_capacity = required;
        return Status::OutputFull;
    }
    let Some(wire_end) = state.wire_position.checked_add(wire as u64) else {
        return Status::Malformed;
    };
    state.pending.extend_from_slice(&pcm[..count]);
    state.position = position;
    result.consumed_frames = count;
    if !ready {
        state.finished |= flush;
        return if flush {
            Status::Complete
        } else {
            Status::NeedInput
        };
    }
    state.pending.resize(wire, 0.0);
    match context.backend.encode(&state.pending, &mut out[..required]) {
        Ok(encoded) if encoded <= required => {
            result.encoded_bytes = encoded;
            let start = state
                .wire_position
                .saturating_sub(state.drain as u64)
                .min(position);
            let end = wire_end.saturating_sub(state.drain as u64).min(position);
            result.source_start_frames = start;
            result.source_frames = (end - start) as u32;
            result.packet_frames = if encoded == 0 { 0 } else { wire as u32 };
            state.wire_position = wire_end;
            state.finished |=
                flush && count == pcm.len() && wire_end >= position + state.drain as u64;
            state.pending.clear();
            Status::Complete
        }
        _ => {
            state.pending.clear();
            Status::Failed
        }
    }
}

unsafe extern "C" fn decode<B: Backend>(
    context: *mut c_void,
    packet: *const u8,
    bytes: usize,
    out: *mut f32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    if !valid(context.cast::<Context<B>>(), 1)
        || !valid(packet, bytes)
        || !valid(out, capacity)
        || !valid(frames, 1)
    {
        return Status::Malformed as i32;
    }
    unsafe {
        frames.write(0);
    }
    let context = unsafe { &mut *context.cast::<Context<B>>() };
    let packet = unsafe { input(packet, bytes) };
    let required = match packet_frames(context, packet) {
        Ok(n) => n as usize,
        Err(status) => return status as i32,
    };
    unsafe {
        frames.write(required);
    }
    if capacity < required {
        return Status::OutputFull as i32;
    }
    if required == 0 {
        return Status::Complete as i32;
    }
    match context
        .backend
        .decode(packet, unsafe { output(out, required) })
    {
        Ok(n) if n == required => Status::Complete as i32,
        result => {
            // Input corruption is recoverable media loss, not a failed provider.
            // Preserve the backend status so the owner can request native PLC.
            unsafe {
                frames.write(0);
            }
            match result {
                Err(status) => status as i32,
                Ok(_) => Status::Failed as i32,
            }
        }
    }
}

unsafe extern "C" fn conceal<B: Backend>(
    context: *mut c_void,
    requested: u32,
    out: *mut f32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    if !valid(context.cast::<Context<B>>(), 1) || !valid(out, capacity) || !valid(frames, 1) {
        return Status::Malformed as i32;
    }
    unsafe {
        frames.write(0);
    }
    let context = unsafe { &mut *context.cast::<Context<B>>() };
    if context.encoder.is_some() {
        return Status::Unsupported as i32;
    }
    let requested = requested as usize;
    if capacity < requested {
        unsafe {
            frames.write(requested);
        }
        return Status::OutputFull as i32;
    }
    if requested == 0 {
        return Status::Complete as i32;
    }
    match context.backend.conceal(unsafe { output(out, requested) }) {
        Ok(()) => {
            unsafe {
                frames.write(requested);
            }
            Status::Complete as i32
        }
        Err(status) => status as i32,
    }
}

unsafe extern "C" fn reset<B: Backend>(context: *mut c_void) -> i32 {
    if !valid(context.cast::<Context<B>>(), 1) {
        return Status::Malformed as i32;
    }
    let context = unsafe { &mut *context.cast::<Context<B>>() };
    if let Some(encoder) = &mut context.encoder {
        encoder.pending.clear();
        encoder.position = 0;
        encoder.wire_position = 0;
        encoder.finished = false;
    }
    match context.backend.reset() {
        Ok(()) => Status::Complete as i32,
        Err(status) => status as i32,
    }
}
unsafe extern "C" fn destroy<B: Backend>(context: *mut c_void) {
    if !context.is_null() {
        unsafe {
            drop(Box::from_raw(context.cast::<Context<B>>()));
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
unsafe fn sized<T>(pointer: *const T) -> bool {
    valid(pointer, 1) && unsafe { pointer.cast::<u32>().read() } as usize >= size_of::<T>()
}
unsafe fn input<'a, T>(pointer: *const T, count: usize) -> &'a [T] {
    if count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(pointer, count) }
    }
}
unsafe fn output<'a, T>(pointer: *mut T, count: usize) -> &'a mut [T] {
    if count == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(pointer, count) }
    }
}

#[cfg(test)]
mod tests;
