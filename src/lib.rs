//! RCode: a small, deterministic circular code with an image renderer.
//!
//! The profiled wire format is protected by Reed-Solomon parity bytes and
//! mapped onto the slots described by [`geometry`]. It is not a QR or
//! WeChat-code implementation.

mod codec;
mod decode;
pub mod geometry;
mod profile;
pub mod render;
pub use profile::{EccLevel, Profile};

pub use decode::{DecodeResult, Landmark, decode_image, decode_luma8};
pub use render::{RenderOptions, Style, render_png, render_svg};

/// Errors reported by the encoder, renderer, and decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Rendering options cannot be used safely.
    InvalidOptions(String),
    /// An image could not be read or does not satisfy the decoder limits.
    InvalidImage(String),
    /// No plausible radial code was found in an image.
    NoCode,
    /// A code was located, but its frame could not be validated or repaired.
    CorruptCode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOptions(message) => write!(f, "invalid options: {message}"),
            Self::InvalidImage(message) => write!(f, "invalid image: {message}"),
            Self::NoCode => f.write_str("no radial code found"),
            Self::CorruptCode(message) => write!(f, "corrupt radial code: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result type used by the public API.
pub type Result<T> = std::result::Result<T, Error>;

/// An encoded symbol.  The payload is retained for callers that want to
/// display metadata; renderers use the protected codeword.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// Original payload bytes.
    pub payload: Vec<u8>,
    pub(crate) codeword: Vec<u8>,
    /// Profile used to encode this symbol.
    pub profile: Profile,
}

/// Encode a payload using one of the twelve line-count and ECC profiles.
pub fn encode(payload: &[u8], profile: Profile) -> Result<Symbol> {
    let codeword = codec::encode(payload, profile)?;
    Ok(Symbol {
        payload: payload.to_vec(),
        codeword,
        profile,
    })
}
