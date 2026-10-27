use super::*;

fn packet(bits: &str) -> Vec<u8> {
    let bits: Vec<_> = bits.chars().filter(|ch| *ch != ' ').collect();
    let mut data = vec![255; bits.len().div_ceil(8)];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit == '0' {
            data[index / 8] &= !(1 << (7 - index % 8));
        }
    }
    data
}

#[test]
fn framing_counts_native_frames_not_control_or_enhancement_bits() {
    for (bits, expected) in [
        ("", Ok(0)),
        ("01111", Ok(0)),
        ("00000", Ok(1)),
        ("1000", Ok(0)),
        ("1000 00000 01111", Ok(1)),
        ("1000 1000 00000 01111", Ok(1)),
        ("01110 0000 0 00000 01111", Ok(1)),
        ("01101 0000 00000 00000 01111", Ok(1)),
        ("01001", Err(Status::Malformed)),
        ("01110", Err(Status::Malformed)),
        ("01101 1111", Err(Status::Malformed)),
        ("00001", Err(Status::Malformed)),
        ("1101", Err(Status::Malformed)),
        ("1001", Err(Status::Malformed)),
        ("1000 1000 1000 00000", Err(Status::Malformed)),
    ] {
        assert_eq!(frames(&packet(bits)), expected, "{bits}");
    }
    // The public provider caps packet bytes to c_int::MAX. The smallest NB
    // frame is five bits, so its count cannot overflow the u32 ABI field.
    assert!((i32::MAX as u64) * 8 / 5 < u64::from(u32::MAX));
}

#[test]
fn reader_truncation_does_not_move_the_cursor() {
    let mut reader = Reader {
        data: &[0xa0],
        position: 0,
    };
    assert_eq!(reader.read(3), Ok(5));
    assert_eq!(reader.skip(5), Ok(()));
    assert_eq!(reader.read(1), Err(Status::Malformed));
    assert_eq!(reader.skip(1), Err(Status::Malformed));
    assert_eq!(reader.read(0), Ok(0));
    assert_eq!(reader.remaining(), 0);
}
