//! RadialCode: a small, deterministic circular code with an image renderer.
//!
//! The v1 wire format is intentionally simple: a fixed 138-byte frame is
//! protected by 64 Reed-Solomon parity bytes and mapped onto the 1,828 slots
//! described by [`geometry`].  It is a project-local format, not a QR or
//! WeChat-code implementation.

pub mod codec;
mod decode;
pub mod geometry;
mod profile;
pub mod render;
pub use profile::{EccLevel, Profile};

pub use decode::{DecodeResult, Landmark, decode_image};
pub use render::{RenderOptions, Style, render_png, render_svg};

/// Maximum unencoded payload size in bytes.
pub const MAX_PAYLOAD: usize = 128;

/// Errors reported by the encoder, renderer, and decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The supplied payload is larger than [`MAX_PAYLOAD`].
    PayloadTooLong(usize),
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
            Self::PayloadTooLong(len) => {
                write!(f, "payload is {len} bytes; maximum is {MAX_PAYLOAD}")
            }
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
    /// None denotes the original 96-line v1 format.
    pub profile: Option<Profile>,
}

/// Encode a payload into a v1 radial symbol.
pub fn encode(payload: &[u8]) -> Result<Symbol> {
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLong(payload.len()));
    }
    let codeword = codec::encode_codeword(payload)?;
    Ok(Symbol {
        payload: payload.to_vec(),
        codeword,
        profile: None,
    })
}

/// Encode a v2 symbol using one of the twelve line-count/ECC profiles.
pub fn encode_with_profile(payload: &[u8], profile: Profile) -> Result<Symbol> {
    let codeword = codec::encode_profile(payload, profile)?;
    Ok(Symbol {
        payload: payload.to_vec(),
        codeword,
        profile: Some(profile),
    })
}
