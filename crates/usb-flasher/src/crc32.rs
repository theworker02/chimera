//! IEEE CRC-32 (ISO-HDLC / GPT) — polynomial 0xEDB88320.

const TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            if c & 1 != 0 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            j += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// GPT / EFI CRC32 of `data` (init 0xFFFF_FFFF, final XOR 0xFFFF_FFFF).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero() {
        assert_eq!(crc32(&[]), 0);
    }

    #[test]
    fn known_vector_123456789() {
        // Standard check: CRC32("123456789") == 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
