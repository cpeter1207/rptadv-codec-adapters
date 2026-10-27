use super::*;
use std::collections::VecDeque;

struct Delayed {
    delay: usize,
    history: VecDeque<f32>,
}
impl Backend for Delayed {
    const COUNT: usize = 0;
    fn profile(_: usize) -> Option<Profile> {
        None
    }
    fn open(config: &Config, _: bool) -> Result<Self, Status> {
        let delay = config.mode_id as usize;
        Ok(Self {
            delay,
            history: vec![0.0; delay].into(),
        })
    }
    fn unit(&self) -> (usize, usize) {
        (4, 4)
    }
    fn drain_frames(&self) -> Result<usize, Status> {
        Ok(self.delay)
    }
    fn inspect(&self, packet: &[u8]) -> Result<u32, Status> {
        Ok(packet.len() as u32)
    }
    fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, Status> {
        for (sample, byte) in pcm.iter().zip(out) {
            self.history.push_back(*sample);
            *byte = (self.history.pop_front().unwrap() * 10.0) as u8;
        }
        Ok(pcm.len())
    }
    fn decode(&mut self, _: &[u8], _: &mut [f32]) -> Result<usize, Status> {
        unreachable!()
    }
    fn reset(&mut self) -> Result<(), Status> {
        self.history = vec![0.0; self.delay].into();
        Ok(())
    }
}

fn context(delay: usize) -> Context<Delayed> {
    let config = Config {
        struct_size: size_of::<Config>() as u32,
        profile_id: 1,
        mode_id: delay as u32,
        packet_frames: 8,
        max_packet_bytes: 64,
    };
    let mut pointer = ptr::null_mut();
    assert_eq!(
        unsafe { open::<Delayed>(&config, &mut pointer, true) },
        Status::Complete as i32
    );
    *unsafe { Box::from_raw(pointer.cast::<Context<Delayed>>()) }
}
fn result() -> EncodeResult {
    unsafe { std::mem::zeroed() }
}

#[test]
fn finite_encoder_tail_preserves_source_coverage_and_delayed_samples() {
    for delay in [0, 3, 12] {
        for count in [1, 3, 4, 8, 9, 16] {
            let mut encoder = context(delay);
            let input: Vec<_> = (1..=count).map(|i| i as f32 / 10.0).collect();
            let mut consumed = 0;
            let mut covered = 0;
            let mut wire = Vec::new();
            for _ in 0..32 {
                let mut packet = [0; 64];
                let mut metadata = result();
                let status = encode_block(
                    &mut encoder,
                    &input[consumed..],
                    &mut packet,
                    consumed == count,
                    &mut metadata,
                );
                assert!(matches!(status, Status::Complete | Status::NeedInput));
                consumed += metadata.consumed_frames;
                if metadata.encoded_bytes == 0 && status == Status::Complete {
                    break;
                }
                if metadata.encoded_bytes != 0 {
                    assert_eq!(metadata.source_start_frames, covered as u64);
                    covered += metadata.source_frames as usize;
                    assert_eq!(metadata.packet_frames as usize, metadata.encoded_bytes);
                    wire.extend_from_slice(&packet[..metadata.encoded_bytes]);
                }
            }
            assert_eq!(consumed, count);
            assert_eq!(covered, count, "delay={delay}, count={count}");
            assert!(
                wire.len() >= count + delay,
                "tail not drained: delay={delay}, count={count}"
            );
            assert_eq!(
                &wire[delay..delay + count],
                &(1..=count as u8).collect::<Vec<_>>()
            );
            for _ in 0..2 {
                let mut metadata = result();
                assert_eq!(
                    encode_block(&mut encoder, &[], &mut [0; 64], true, &mut metadata),
                    Status::Complete
                );
                assert_eq!(metadata.encoded_bytes, 0);
            }
        }
    }
}

#[test]
fn finite_tail_capacity_retry_and_reset_do_not_consume_or_repeat_history() {
    let mut short = context(12);
    let mut reference = context(12);
    for encoder in [&mut short, &mut reference] {
        let mut metadata = result();
        assert_eq!(
            encode_block(encoder, &[0.5; 8], &mut [0; 64], false, &mut metadata),
            Status::Complete
        );
        assert_eq!(metadata.source_frames, 0);
    }
    let mut metadata = result();
    assert_eq!(
        encode_block(&mut short, &[], &mut [0; 1], true, &mut metadata),
        Status::OutputFull
    );
    assert_eq!((metadata.consumed_frames, metadata.encoded_bytes), (0, 0));
    assert_eq!(metadata.required_capacity, 8);
    let (mut actual, mut expected) = ([0; 64], [0; 64]);
    let mut expected_metadata = result();
    assert_eq!(
        encode_block(&mut short, &[], &mut actual, true, &mut metadata),
        Status::Complete
    );
    assert_eq!(
        encode_block(
            &mut reference,
            &[],
            &mut expected,
            true,
            &mut expected_metadata
        ),
        Status::Complete
    );
    assert_eq!(actual, expected);
    assert_eq!(
        (metadata.source_start_frames, metadata.source_frames),
        (0, 4)
    );
    assert_eq!(
        unsafe { reset::<Delayed>((&mut short as *mut Context<Delayed>).cast()) },
        Status::Complete as i32
    );
    let mut fresh = context(12);
    encode_block(&mut short, &[0.8; 8], &mut actual, false, &mut metadata);
    encode_block(
        &mut fresh,
        &[0.8; 8],
        &mut expected,
        false,
        &mut expected_metadata,
    );
    assert_eq!(actual, expected);
    assert_eq!(metadata.source_start_frames, 0);
}
