use thiserror::Error;

pub type SwfResult<T> = Result<T, SwfError>;

#[derive(Debug, Error)]
pub enum SwfError {
    #[error("invalid SWF: {0}")]
    Invalid(String),

    #[error("shape uses {0} fill styles; at most 65534 are supported by the SWF format")]
    TooManyFills(usize),

    #[error("shape uses {0} line styles; at most 65534 are supported by the SWF format")]
    TooManyLines(usize),

    #[error("value out of range: {0}")]
    Range(String),

    #[error("zlib error: {0}")]
    Zlib(#[from] std::io::Error),

    #[error("unsupported SWF signature '{0}'")]
    Signature(String),
}
