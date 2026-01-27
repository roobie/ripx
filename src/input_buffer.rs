use std::io::{self, Read};

/// Fixed-size input buffer with sliding window semantics.
pub struct InputBuffer {
    buf: Box<[u8]>,
    start: usize,
    end: usize,
    eof: bool,
}

impl InputBuffer {
    pub fn with_capacity(cap: usize) -> Self {
        let mut v = Vec::with_capacity(cap);
        v.resize(cap, 0);
        InputBuffer { buf: v.into_boxed_slice(), start: 0, end: 0, eof: false }
    }

    pub fn len(&self) -> usize { self.end - self.start }

    pub fn capacity(&self) -> usize { self.buf.len() }

    pub fn as_slice(&self) -> &[u8] { &self.buf[self.start..self.end] }

    pub fn clear(&mut self) { self.start = 0; self.end = 0; self.eof = false; }

    /// Consume `n` bytes from the front of the buffer.
    pub fn consume(&mut self, n: usize) {
        let n = n.min(self.len());
        self.start += n;
        if self.start == self.end {
            // reset indices to avoid unbounded growth
            self.start = 0;
            self.end = 0;
        }
    }

    /// Ensure at least `n` bytes are available, reading from `reader` as needed.
    /// Returns Ok(true) if at least `n` bytes are available, Ok(false) on EOF with fewer bytes.
    pub fn ensure<R: Read>(&mut self, n: usize, reader: &mut R) -> io::Result<bool> {
        while self.len() < n && !self.eof {
            let _ = self.fill_from(reader)?;
            if self.len() >= n { break; }
        }
        Ok(self.len() >= n)
    }

    /// Fill buffer from reader, shifting data to front if necessary.
    /// Returns number of bytes read (0 on EOF).
    pub fn fill_from<R: Read>(&mut self, reader: &mut R) -> io::Result<usize> {
        if self.eof { return Ok(0); }
        // If there's no space at the end, but there is consumed space at front, slide data.
        if self.end == self.buf.len() {
            if self.start > 0 {
                let len = self.len();
                // safe because regions don't overlap in this direction
                self.buf.copy_within(self.start..self.end, 0);
                self.start = 0;
                self.end = len;
            } else {
                // buffer full and nothing consumed: cannot read more
                return Ok(0);
            }
        }
        let read_into = &mut self.buf[self.end..];
        match reader.read(read_into) {
            Ok(0) => { self.eof = true; Ok(0) }
            Ok(n) => { self.end += n; Ok(n) }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn fill_and_slice() {
        let data = b"abcdefghij";
        let mut cursor = Cursor::new(&data[..]);
        let mut ib = InputBuffer::with_capacity(8);
        let n = ib.fill_from(&mut cursor).expect("read");
        assert!(n > 0);
        assert_eq!(ib.as_slice(), &data[..n]);
    }

    #[test]
    fn consume_and_slide() {
        let data = b"0123456789";
        let mut cursor = Cursor::new(&data[..]);
        let mut ib = InputBuffer::with_capacity(6);
        // read initial chunk
        let _ = ib.fill_from(&mut cursor).expect("read1");
        // consume some bytes
        ib.consume(4);
        // read more, which should cause sliding
        let _ = ib.fill_from(&mut cursor).expect("read2");
        // ensure we still have a valid contiguous slice
        let s = ib.as_slice();
        assert!(s.len() > 0);
    }
}
