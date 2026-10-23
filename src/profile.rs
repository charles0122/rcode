//! v3 line counts and Reed–Solomon correction budgets.
use crate::{Error, Result, geometry};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EccLevel {
    L,
    M,
    Q,
    H,
}
impl EccLevel {
    pub const ALL: [Self; 4] = [Self::L, Self::M, Self::Q, Self::H];
    pub fn percent(self) -> usize {
        [10, 15, 25, 35][self.id() as usize]
    }
    pub(crate) fn id(self) -> u8 {
        match self {
            Self::L => 0,
            Self::M => 1,
            Self::Q => 2,
            Self::H => 3,
        }
    }
}
impl fmt::Display for EccLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(["L", "M", "Q", "H"][self.id() as usize])
    }
}
impl FromStr for EccLevel {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "L" => Ok(Self::L),
            "M" => Ok(Self::M),
            "Q" => Ok(Self::Q),
            "H" => Ok(Self::H),
            _ => Err(Error::InvalidOptions("纠错等级必须为 L、M、Q 或 H".into())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    spokes: usize,
    pub level: EccLevel,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            spokes: 72,
            level: EccLevel::M,
        }
    }
}
impl Profile {
    pub fn new(spokes: usize, level: EccLevel) -> Result<Self> {
        if ![36, 54, 72].contains(&spokes) {
            return Err(Error::InvalidOptions("线数必须为 36、54 或 72".into()));
        }
        Ok(Self { spokes, level })
    }
    pub fn spokes(self) -> usize {
        self.spokes
    }
    pub fn codeword_len(self) -> usize {
        geometry::slots_for_spokes(self.spokes).len() / 8
    }
    pub fn correctable_bytes(self) -> usize {
        (self.codeword_len() * self.level.percent()).div_ceil(100)
    }
    pub fn parity_bytes(self) -> usize {
        self.correctable_bytes() * 2
    }
    pub fn capacity(self) -> usize {
        self.codeword_len() - self.parity_bytes() - 12
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{codec, encode};

    #[test]
    fn capacities_and_exact_error_limits_for_all_twelve_profiles() {
        for (spokes, expected) in [
            (36, [57, 47, 31, 13]),
            (54, [92, 78, 52, 26]),
            (72, [124, 108, 74, 38]),
        ] {
            for (level, capacity) in EccLevel::ALL.into_iter().zip(expected) {
                let p = Profile::new(spokes, level).unwrap();
                assert_eq!(p.capacity(), capacity);
                let payload = vec![0xa5; capacity];
                let mut word = codec::encode(&payload, p).unwrap();
                assert_eq!(word.len(), p.codeword_len());
                for (i, byte) in word.iter_mut().take(p.correctable_bytes()).enumerate() {
                    *byte ^= (i + 1) as u8;
                }
                let (actual, corrected) = codec::decode(&word, p).unwrap();
                assert_eq!(actual, payload);
                assert_eq!(corrected, p.correctable_bytes());
                assert!(encode(&vec![0; capacity + 1], p).is_err());
                let wrong = Profile::new(
                    spokes,
                    if level == EccLevel::L {
                        EccLevel::H
                    } else {
                        EccLevel::L
                    },
                )
                .unwrap();
                assert!(codec::decode(&word, wrong).is_err());
            }
        }
        assert!(Profile::new(96, EccLevel::M).is_err());
        assert!("X".parse::<EccLevel>().is_err());
    }
}
