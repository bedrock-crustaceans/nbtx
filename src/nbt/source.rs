//! The reader every binary decode runs through, carrying its [`DecodeOptions`] budget.

use std::fmt;
use std::io::{self, Read};

use crate::{DecodeOptions, Error, max_depth_exceeded_at};

/// Raised through `io::Error` when the byte budget runs out, then mapped to
/// [`Error::InputSizeExceeded`] by `From<io::Error>`.
#[derive(Debug)]
pub(crate) struct InputExhausted(pub(crate) usize);

impl fmt::Display for InputExhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NBT input exceeds the {}-byte decode limit", self.0)
    }
}

impl std::error::Error for InputExhausted {}

pub(crate) struct Source<R> {
    inner: R,
    options: DecodeOptions,
    bytes_left: usize,
    nodes_left: usize,
}

impl<R: Read> Source<R> {
    pub(crate) fn new(inner: R, options: DecodeOptions) -> Self {
        Self {
            inner,
            options,
            bytes_left: options.max_bytes,
            nodes_left: options.max_nodes,
        }
    }
}

impl<R: Read> Read for Source<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.bytes_left == 0 {
            return Err(io::Error::other(InputExhausted(self.options.max_bytes)));
        }
        let len = buf.len().min(self.bytes_left);
        let read = self.inner.read(&mut buf[..len])?;
        self.bytes_left -= read;
        Ok(read)
    }
}

/// What the decoders need from their reader beyond the bytes themselves.
pub(crate) trait Budget: Read {
    /// Errors if `depth` containers are already entered. Call before recursing.
    fn enter(&mut self, depth: usize) -> Result<(), Error>;

    /// Counts one decoded value against the node budget.
    fn charge(&mut self) -> Result<(), Error>;
}

impl<R: Read> Budget for Source<R> {
    fn enter(&mut self, depth: usize) -> Result<(), Error> {
        if depth >= self.options.max_depth {
            return Err(max_depth_exceeded_at(self.options.max_depth));
        }
        Ok(())
    }

    fn charge(&mut self) -> Result<(), Error> {
        if self.nodes_left == 0 {
            return Err(Error::NodeCountExceeded {
                max: self.options.max_nodes,
            });
        }
        self.nodes_left -= 1;
        Ok(())
    }
}
