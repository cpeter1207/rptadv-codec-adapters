//! Speex raw bitstream lengths (Speex 1.2.1/Asterisk 20.21.0 mode tables).
use rptadv_codec_contract::Status;

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}
impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.data.len() * 8 - self.position
    }
    fn skip(&mut self, count: usize) -> Result<(), Status> {
        if count > self.remaining() {
            return Err(Status::Malformed);
        }
        self.position += count;
        Ok(())
    }
    fn read(&mut self, count: usize) -> Result<usize, Status> {
        if count > self.remaining() {
            return Err(Status::Malformed);
        }
        let mut value = 0;
        for bit in self.position..self.position + count {
            value = (value << 1) | usize::from((self.data[bit / 8] >> (7 - bit % 8)) & 1);
        }
        self.position += count;
        Ok(value)
    }
}

pub(super) fn frames(packet: &[u8]) -> Result<u32, Status> {
    const NB: [usize; 9] = [5, 43, 119, 160, 220, 300, 364, 492, 79];
    const WB: [usize; 5] = [4, 36, 112, 192, 352];
    const INBAND: [usize; 16] = [1, 1, 4, 4, 4, 4, 4, 4, 8, 8, 16, 16, 32, 32, 64, 64];
    let mut reader = Reader {
        data: packet,
        position: 0,
    };
    let mut count = 0u32;
    while reader.remaining() >= 5 {
        let mut wide = 0;
        while reader.remaining() >= 5 {
            if reader.read(1)? == 0 {
                reader.position -= 1;
                break;
            }
            wide += 1;
            let mode = reader.read(3)?;
            if wide > 2 {
                return Err(Status::Malformed);
            }
            reader.skip(WB.get(mode).ok_or(Status::Malformed)? - 4)?;
        }
        if reader.remaining() < 5 {
            break;
        }
        match reader.read(5)? {
            15 => break,
            14 => {
                let mode = reader.read(4)?;
                reader.skip(INBAND[mode])?;
            }
            13 => {
                let bytes = reader.read(4)?;
                reader.skip(5 + bytes * 8)?;
            }
            mode @ 0..=8 => {
                reader.skip(NB[mode] - 5)?;
                count = count.checked_add(1).ok_or(Status::Malformed)?;
            }
            _ => return Err(Status::Malformed),
        }
    }
    Ok(count)
}
