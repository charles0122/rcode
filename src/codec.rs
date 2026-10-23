//! Profiled frame and Reed–Solomon codeword handling.

use reed_solomon::{Decoder, Encoder};

use crate::{Error, Profile, Result};

const MAGIC: [u8; 4] = *b"RADC";
const VERSION: u8 = 3;

pub(crate) fn encode(payload: &[u8], profile: Profile) -> Result<Vec<u8>> {
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
    frame[4] = VERSION;
    frame[5] = profile.spokes() as u8;
    frame[6] = profile.level.id();
    frame[7] = payload.len() as u8;
    frame[8..8 + payload.len()].copy_from_slice(payload);
    let end = frame.len() - 4;
    let crc = crc32fast::hash(&frame[..end]);
    frame[end..].copy_from_slice(&crc.to_be_bytes());
    Ok(Encoder::new(profile.parity_bytes()).encode(&frame).to_vec())
}

pub(crate) fn decode(word: &[u8], profile: Profile) -> Result<(Vec<u8>, usize)> {
    if word.len() != profile.codeword_len() {
        return Err(Error::CorruptCode("wrong profile length".into()));
    }
    let (corrected, count) = Decoder::new(profile.parity_bytes())
        .correct_err_count(word, None)
        .map_err(|_| Error::CorruptCode("profile correction failed".into()))?;
    let frame = corrected.data();
    let end = frame.len() - 4;
    let len = frame[7] as usize;
    if frame[..4] != MAGIC
        || frame[4] != VERSION
        || frame[5] != profile.spokes() as u8
        || frame[6] != profile.level.id()
        || len > profile.capacity()
        || frame[8 + len..end].iter().any(|byte| *byte != 0)
        || crc32fast::hash(&frame[..end]) != u32::from_be_bytes(frame[end..].try_into().unwrap())
    {
        return Err(Error::CorruptCode(
            "invalid profile header, padding or CRC".into(),
        ));
    }
    Ok((frame[8..8 + len].to_vec(), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protect(frame: &[u8], profile: Profile) -> Vec<u8> {
        Encoder::new(profile.parity_bytes()).encode(frame).to_vec()
    }

    fn refresh_crc(frame: &mut [u8]) {
        let end = frame.len() - 4;
        let crc = crc32fast::hash(&frame[..end]);
        frame[end..].copy_from_slice(&crc.to_be_bytes());
    }

    #[test]
    fn version_three_frame_is_current_and_invalid_fields_are_rejected() {
        let profile = Profile::default();
        let word = encode(b"frame", profile).unwrap();
        let (decoded, _) = Decoder::new(profile.parity_bytes())
            .correct_err_count(&word, None)
            .unwrap();
        let valid = decoded.data().to_vec();
        assert_eq!(valid[4], 3);
        assert_eq!(decode(&word, profile).unwrap().0, b"frame");

        let mut bad_version = valid.clone();
        bad_version[4] = 2;
        refresh_crc(&mut bad_version);
        assert!(decode(&protect(&bad_version, profile), profile).is_err());

        let mut bad_padding = valid.clone();
        bad_padding[8 + b"frame".len()] = 1;
        refresh_crc(&mut bad_padding);
        assert!(decode(&protect(&bad_padding, profile), profile).is_err());

        let mut bad_crc = valid;
        *bad_crc.last_mut().unwrap() ^= 1;
        assert!(decode(&protect(&bad_crc, profile), profile).is_err());
    }
}
