//! v1 frame and Reed-Solomon codeword handling.

use crc32fast::Hasher;
use reed_solomon::{Decoder, Encoder};

use crate::{Error, MAX_PAYLOAD, Result};

/// Four-byte protocol identifier.
pub const MAGIC: [u8; 4] = *b"RADC";
/// Wire-format version.
pub const VERSION: u8 = 1;
/// Fixed unprotected frame length.
pub const FRAME_LEN: usize = 4 + 1 + 1 + MAX_PAYLOAD + 4;
/// Reed-Solomon parity length.
pub const ECC_LEN: usize = 64;
/// Protected codeword length.
pub const CODEWORD_LEN: usize = FRAME_LEN + ECC_LEN;

fn frame_for_payload(payload: &[u8]) -> Result<[u8; FRAME_LEN]> {
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLong(payload.len()));
    }

    let mut frame = [0u8; FRAME_LEN];
    frame[..4].copy_from_slice(&MAGIC);
    frame[4] = VERSION;
    frame[5] = payload.len() as u8;
    frame[6..6 + payload.len()].copy_from_slice(payload);

    let mut hasher = Hasher::new();
    hasher.update(&frame[..FRAME_LEN - 4]);
    frame[FRAME_LEN - 4..].copy_from_slice(&hasher.finalize().to_be_bytes());
    Ok(frame)
}

fn encode_frame(frame: &[u8; FRAME_LEN]) -> Vec<u8> {
    let encoded = Encoder::new(ECC_LEN).encode(frame);
    encoded.to_vec()
}

/// Encode a payload into the 202-byte protected codeword.
pub fn encode_codeword(payload: &[u8]) -> Result<Vec<u8>> {
    Ok(encode_frame(&frame_for_payload(payload)?))
}

/// Decode, repair, and validate one v1 codeword.
///
/// The returned count is the number of corrected codeword bytes reported by
/// the Reed-Solomon decoder.  A CRC and zero-padding check follows correction
/// so a mathematically plausible but wrong frame is rejected.
pub fn decode_codeword(codeword: &[u8]) -> Result<(Vec<u8>, usize)> {
    if codeword.len() != CODEWORD_LEN {
        return Err(Error::CorruptCode(format!(
            "codeword has {} bytes; expected {CODEWORD_LEN}",
            codeword.len()
        )));
    }

    let decoder = Decoder::new(ECC_LEN);
    let (corrected, corrected_count) =
        decoder.correct_err_count(codeword, None).map_err(|error| {
            Error::CorruptCode(format!("reed-solomon correction failed: {error:?}"))
        })?;
    let frame = corrected.data();

    if frame.len() != FRAME_LEN {
        return Err(Error::CorruptCode(
            "repaired frame has the wrong length".into(),
        ));
    }
    if frame[..4] != MAGIC {
        return Err(Error::CorruptCode("bad magic".into()));
    }
    if frame[4] != VERSION {
        return Err(Error::CorruptCode(format!(
            "unsupported version {}",
            frame[4]
        )));
    }

    let payload_len = frame[5] as usize;
    if payload_len > MAX_PAYLOAD {
        return Err(Error::CorruptCode(format!(
            "payload length {payload_len} exceeds {MAX_PAYLOAD}"
        )));
    }
    if frame[6 + payload_len..FRAME_LEN - 4]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(Error::CorruptCode("non-zero payload padding".into()));
    }

    let mut hasher = Hasher::new();
    hasher.update(&frame[..FRAME_LEN - 4]);
    let expected_crc = u32::from_be_bytes(frame[FRAME_LEN - 4..].try_into().unwrap());
    if hasher.finalize() != expected_crc {
        return Err(Error::CorruptCode("CRC32 mismatch".into()));
    }

    Ok((frame[6..6 + payload_len].to_vec(), corrected_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_vector_and_roundtrip() {
        let frame = frame_for_payload(b"").unwrap();
        assert_eq!(&frame[..6], b"RADC\x01\x00");
        assert_eq!(&frame[FRAME_LEN - 4..], &[0x53, 0x56, 0xa5, 0xb7]);
        let word = encode_codeword(b"hello").unwrap();
        assert_eq!(word.len(), CODEWORD_LEN);
        let (decoded, fixed) = decode_codeword(&word).unwrap();
        assert_eq!(decoded, b"hello");
        assert_eq!(fixed, 0);
    }

    #[test]
    fn roundtrip_boundary_payloads() {
        for payload in [
            Vec::new(),
            "你好，radial".as_bytes().to_vec(),
            vec![0x5a; MAX_PAYLOAD],
        ] {
            let word = encode_codeword(&payload).unwrap();
            assert_eq!(decode_codeword(&word).unwrap().0, payload);
        }
        assert!(matches!(
            encode_codeword(&[0u8; MAX_PAYLOAD + 1]),
            Err(Error::PayloadTooLong(_))
        ));
    }

    #[test]
    fn repairs_one_sixteen_and_thirty_two_bad_bytes() {
        let original = encode_codeword(b"repair me").unwrap();
        for count in [1usize, 16, 32] {
            let mut damaged = original.clone();
            for index in 0..count {
                let slot = index * 3 % damaged.len();
                damaged[slot] ^= (index as u8).wrapping_add(1);
            }
            let (decoded, fixed) = decode_codeword(&damaged).unwrap();
            assert_eq!(decoded, b"repair me");
            assert_eq!(fixed, count);
        }
    }

    #[test]
    fn rejects_damage_beyond_the_correction_bound() {
        let mut damaged = encode_codeword(b"damage beyond correction").unwrap();
        for (index, byte) in damaged.iter_mut().take(64).enumerate() {
            *byte ^= (index + 1) as u8;
        }
        assert!(decode_codeword(&damaged).is_err());
    }

    #[test]
    fn rejects_bad_crc_version_padding_and_length() {
        let payload = b"frame";
        let valid = frame_for_payload(payload).unwrap();

        let mut bad_crc = valid;
        bad_crc[FRAME_LEN - 1] ^= 1;
        assert!(matches!(
            decode_codeword(&encode_frame(&bad_crc)),
            Err(Error::CorruptCode(message)) if message.contains("CRC")
        ));

        let mut bad_version = valid;
        bad_version[4] = VERSION + 1;
        assert!(matches!(
            decode_codeword(&encode_frame(&bad_version)),
            Err(Error::CorruptCode(message)) if message.contains("version")
        ));

        let mut bad_padding = valid;
        bad_padding[6 + payload.len()] = 1;
        assert!(matches!(
            decode_codeword(&encode_frame(&bad_padding)),
            Err(Error::CorruptCode(message)) if message.contains("padding")
        ));

        assert!(matches!(
            decode_codeword(&[0; CODEWORD_LEN - 1]),
            Err(Error::CorruptCode(message)) if message.contains("codeword")
        ));
    }
}

/// Encode the v2 profile header and payload inside one protected RS frame.
pub(crate) fn encode_profile(payload: &[u8], profile: crate::Profile) -> Result<Vec<u8>> {
    let capacity = profile.capacity();
    if payload.len() > capacity {
        return Err(Error::InvalidOptions(format!(
            "{} 线 {} 档最多 {} 字节，当前 {} 字节",
            profile.spokes(),
            profile.level,
            capacity,
            payload.len()
        )));
    }
    let mut frame = vec![0; profile.codeword_len() - profile.parity_bytes()];
    frame[..4].copy_from_slice(&MAGIC);
    frame[4] = 2;
    frame[5] = profile.spokes() as u8;
    frame[6] = profile.level.id();
    frame[7] = payload.len() as u8;
    frame[8..8 + payload.len()].copy_from_slice(payload);
    let end = frame.len() - 4;
    let crc = crc32fast::hash(&frame[..end]);
    frame[end..].copy_from_slice(&crc.to_be_bytes());
    Ok(Encoder::new(profile.parity_bytes()).encode(&frame).to_vec())
}

pub(crate) fn decode_profile(word: &[u8], profile: crate::Profile) -> Result<(Vec<u8>, usize)> {
    if word.len() != profile.codeword_len() {
        return Err(Error::CorruptCode("wrong profile length".into()));
    }
    let (corrected, count) = Decoder::new(profile.parity_bytes())
        .correct_err_count(word, None)
        .map_err(|_| Error::CorruptCode("profile correction failed".into()))?;
    let f = corrected.data();
    let end = f.len() - 4;
    let len = f[7] as usize;
    if f[..4] != MAGIC
        || f[4] != 2
        || f[5] != profile.spokes() as u8
        || f[6] != profile.level.id()
        || len > profile.capacity()
        || f[8 + len..end].iter().any(|b| *b != 0)
        || crc32fast::hash(&f[..end]) != u32::from_be_bytes(f[end..].try_into().unwrap())
    {
        return Err(Error::CorruptCode(
            "invalid v2 header, padding or CRC".into(),
        ));
    }
    Ok((f[8..8 + len].to_vec(), count))
}
