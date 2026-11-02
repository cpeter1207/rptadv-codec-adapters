//! Dependency-return and arithmetic boundary checks for the shared ABI owner.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};

thread_local! {
    static FAIL_ALLOCATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct Allocator;
// SAFETY: delegate every layout/ownership rule to System, except one requested
// test-thread allocation failure. Other test threads are unaffected.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if FAIL_ALLOCATION
            .try_with(|flag| flag.replace(false))
            .unwrap_or(false)
        {
            ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

struct Probe(u32);
impl Backend for Probe {
    const COUNT: usize = 0;
    fn profile(_: usize) -> Option<Profile> {
        None
    }
    fn open(config: &Config, _: bool) -> Result<Self, Status> {
        if config.profile_id == 0 {
            Err(Status::Unsupported)
        } else {
            Ok(Self(config.mode_id))
        }
    }
    fn unit(&self) -> (usize, usize) {
        match self.0 {
            1 => (0, 1),
            2 => (1, 0),
            3 => (1, usize::MAX),
            13 => (2, 1),
            _ => (1, 1),
        }
    }
    fn drain_frames(&self) -> Result<usize, Status> {
        if self.0 == 4 {
            Err(Status::Failed)
        } else {
            Ok(0)
        }
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        if self.0 == 5 {
            Err(Status::Malformed)
        } else {
            Ok(packet.len() as u32)
        }
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        match self.0 {
            6 => Err(Status::Failed),
            7 => Ok(out.len() + 1),
            12 => Ok(0),
            _ => {
                out.fill(42);
                Ok(pcm.len())
            }
        }
    }
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, Status> {
        match self.0 {
            8 => Err(Status::Malformed),
            9 => Ok(0),
            _ => {
                out.fill(0.25);
                Ok(packet.len())
            }
        }
    }
    fn conceal(&mut self, out: &mut [f32]) -> Result<(), Status> {
        if self.0 == 11 {
            Err(Status::Failed)
        } else {
            out.fill(0.25);
            Ok(())
        }
    }
    fn reset(&mut self) -> Result<(), Status> {
        if self.0 == 10 {
            Err(Status::Failed)
        } else {
            Ok(())
        }
    }
}

fn config(mode: u32) -> Config {
    Config {
        struct_size: size_of::<Config>() as u32,
        profile_id: 1,
        mode_id: mode,
        packet_frames: 8,
        max_packet_bytes: 64,
    }
}
fn context(mode: u32, encoder: bool) -> Box<Context<Probe>> {
    let mut pointer = ptr::null_mut();
    assert_eq!(
        unsafe { open::<Probe>(&config(mode), &mut pointer, encoder) },
        0
    );
    unsafe { Box::from_raw(pointer.cast()) }
}

#[test]
fn contract_rejects_invalid_backend_units_and_reports_allocation_failure() {
    let mut pointer = ptr::null_mut();
    for mode in 1..=4 {
        assert_eq!(
            unsafe { open::<Probe>(&config(mode), &mut pointer, true) },
            if mode == 4 { 5 } else { 3 }
        );
        assert!(pointer.is_null());
    }
    let mut invalid = config(0);
    invalid.profile_id = 0;
    assert_eq!(unsafe { open::<Probe>(&invalid, &mut pointer, true) }, 4);
    FAIL_ALLOCATION.with(|flag| flag.set(true));
    let status = unsafe { open::<Probe>(&config(0), &mut pointer, true) };
    assert_eq!(status, 5);
    assert!(pointer.is_null());
    let mut partial = config(13);
    partial.packet_frames = 3;
    assert_eq!(unsafe { open::<Probe>(&partial, &mut pointer, true) }, 3);
}

#[test]
fn contract_runtime_table_and_partial_final_input_preserve_progress() {
    let table = descriptor::<Probe>();
    assert_eq!(table.struct_size as usize, size_of::<Descriptor>());
    assert_eq!(table.profile_count.unwrap()(), 0);
    for mode in [0, 12] {
        let mut owner = context(mode, true);
        let mut result: EncodeResult = unsafe { std::mem::zeroed() };
        assert_eq!(
            encode_block(&mut owner, &[0.25; 16], &mut [0; 64], true, &mut result),
            Status::Complete
        );
        assert_eq!(result.consumed_frames, 8);
        assert_eq!(result.encoded_bytes, if mode == 12 { 0 } else { 8 });
        assert_eq!(result.packet_frames, if mode == 12 { 0 } else { 8 });
        assert!(!owner.encoder.as_ref().unwrap().finished);
        assert_eq!(
            encode_block(&mut owner, &[0.25; 8], &mut [0; 64], true, &mut result),
            Status::Complete
        );
        assert!(owner.encoder.as_ref().unwrap().finished);
    }
}

#[test]
fn contract_source_and_wire_overflow_do_not_consume_input() {
    for fault in 0..3 {
        let mut owner = context(0, true);
        let state = owner.encoder.as_mut().unwrap();
        match fault {
            0 => state.position = u64::MAX,
            1 => state.drain = usize::MAX,
            _ => state.wire_position = u64::MAX,
        }
        let mut result: EncodeResult = unsafe { std::mem::zeroed() };
        assert_eq!(
            encode_block(&mut owner, &[0.25; 8], &mut [0; 64], false, &mut result),
            Status::Malformed
        );
        assert_eq!(result.consumed_frames, 0);
        assert!(owner.encoder.as_ref().unwrap().pending.is_empty());
    }
}

#[test]
fn contract_failed_backend_results_are_not_reported_as_success() {
    for mode in [6, 7] {
        let mut owner = context(mode, true);
        let mut result: EncodeResult = unsafe { std::mem::zeroed() };
        assert_eq!(
            encode_block(&mut owner, &[0.25; 8], &mut [0; 64], false, &mut result),
            Status::Failed
        );
        assert!(owner.encoder.as_ref().unwrap().pending.is_empty());
        assert_eq!(result.encoded_bytes, 0);
    }
    for (mode, status) in [(5, 3), (8, 3), (9, 5), (0, 0)] {
        let mut owner = context(mode, false);
        let pointer = (&mut *owner as *mut Context<Probe>).cast();
        let mut count = 999;
        assert_eq!(
            unsafe {
                decode::<Probe>(
                    pointer,
                    [1_u8; 8].as_ptr(),
                    8,
                    [0.0_f32; 8].as_mut_ptr(),
                    8,
                    &mut count,
                )
            },
            status
        );
        assert_eq!(count, if status == 0 { 8 } else { 0 });
    }
    let mut owner = context(10, false);
    assert_eq!(
        unsafe { reset::<Probe>((&mut *owner as *mut Context<Probe>).cast()) },
        5
    );
    let mut owner = context(11, false);
    let mut count = 999;
    assert_eq!(
        unsafe {
            conceal::<Probe>(
                (&mut *owner as *mut Context<Probe>).cast(),
                8,
                [0.0_f32; 8].as_mut_ptr(),
                8,
                &mut count,
            )
        },
        5
    );
    assert_eq!(count, 0);
}
