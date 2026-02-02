use std::io::{self, Read};
use crate::tokenizer::{scan_name, parse_attributes, find_sequence};
use memchr;

pub struct FilteringScanner<R: Read> {
    reader: R,
    buf: Vec<u8>,
    pos: usize,
    len: usize,
}

impl<R: Read> FilteringScanner<R> {
    pub fn new(reader: R) -> Self {
        let cap = 8 * 1024;
        FilteringScanner {
            reader,
            buf: vec![0u8; cap],
            pos: 0,
            len: 0,
        }
    }

    fn refill(&mut self) -> io::Result<()> {
        if self.pos > 0 && self.pos < self.len {
            let remaining = self.len - self.pos;
            self.buf.copy_within(self.pos..self.len, 0);
            self.pos = 0;
            self.len = remaining;
        } else if self.pos >= self.len {
            self.pos = 0;
            self.len = 0;
        }
        let n = self.reader.read(&mut self.buf[self.len..])?;
        self.len += n;
        Ok(())
    }

    pub fn find_start_tag(&mut self, name: &[u8]) -> io::Result<Option<usize>> {
        if name.is_empty() {
            return Ok(None);
        }
        let mut pat = Vec::with_capacity(name.len() + 1);
        pat.push(b'<');
        pat.extend_from_slice(name);

        loop {
            if self.pos >= self.len {
                self.refill()?;
                if self.pos >= self.len {
                    return Ok(None);
                }
            }
            if let Some(rel) = memchr::memchr(b'<', &self.buf[self.pos..self.len]) {
                let i = self.pos + rel;
                let need = 1 + name.len();
                if i + need > self.len {
                    self.refill()?;
                    if i + need > self.len {
                        return Ok(None);
                    }
                }
                if &self.buf[i..i + need] == pat.as_slice() {
                    return Ok(Some(i));
                } else {
                    self.pos = i + 1;
                    continue;
                }
            } else {
                self.pos = self.len;
                self.refill()?;
                if self.pos >= self.len {
                    return Ok(None);
                }
            }
        }
    }

    /// Extract the full element (including nested content) starting at buffer offset `start`.
    /// Returns a Vec<u8> containing the element bytes. This will read from the underlying
    /// reader as needed. The start offset must point at a '<' byte beginning the start-tag.
    pub fn extract_full_element(&mut self, start: usize, name: &[u8]) -> io::Result<Option<Vec<u8>>> {
        // We'll accumulate into out Vec
        let mut out: Vec<u8> = Vec::new();
        let mut read_pos = start;
        let mut depth: i32 = 0;

        loop {
            // ensure we have data at read_pos
            if read_pos >= self.len {
                self.refill()?;
                if read_pos >= self.len {
                    return Ok(None);
                }
            }

            // process from read_pos to available end
            let slice = &self.buf[read_pos..self.len];
            // search for next '<'
            if let Some(rel) = memchr::memchr(b'<', slice) {
                let i = read_pos + rel;
                // copy up to next '<'
                out.extend_from_slice(&self.buf[read_pos..i]);
                // ensure we have bytes following '<' to inspect
                if i + 1 >= self.len {
                    self.refill()?;
                    if i + 1 >= self.len {
                        // incomplete
                        return Ok(None);
                    }
                }
                // determine tag type
                let next = self.buf[i + 1];
                if next == b'/' {
                    // possible end tag
                    // attempt to parse name
                    let rest = &self.buf[i + 2..self.len];
                    let (nlen, _delim) = scan_name(rest, name.len());
                    // ensure full name bytes are present
                    if i + 2 + nlen > self.len {
                        self.refill()?;
                        // if still incomplete, treat as incomplete
                        if i + 2 + nlen > self.len {
                            return Ok(None);
                        }
                    }
                    let end_name = &self.buf[i + 2..i + 2 + nlen];
                    // append the '<' and following bytes up to end of name (may not include '>')
                    // we'll copy until '>' when found
                    // find closing '>'
                    if let Some(gt_rel) = memchr::memchr(b'>', &self.buf[i + 2..self.len]) {
                        let gt = i + 2 + gt_rel;
                        out.extend_from_slice(&self.buf[i..gt + 1]);
                        read_pos = gt + 1;
                        if end_name == name {
                            depth -= 1;
                            if depth == 0 {
                                // matched closing of our starting element -> finish
                                // advance scanner position to read_pos so subsequent searches continue
                                self.pos = read_pos;
                                return Ok(Some(out));
                            }
                        }
                        continue;
                    } else {
                        // need more bytes to find '>'
                        self.refill()?;
                        continue;
                    }
                } else if next == b'!' {
                    // comment or CDATA or DOCTYPE; try to consume accordingly
                    if self.buf.len() >= i + 4 && &self.buf[i..i + 4] == b"<!--" {
                        // comment: find -->
                        if let Some(end_rel) = find_sequence(&self.buf[i..self.len], b"-->") {
                            let end = i + end_rel + 3;
                            out.extend_from_slice(&self.buf[i..end]);
                            read_pos = end;
                            continue;
                        } else {
                            self.refill()?;
                            continue;
                        }
                    } else if self.buf.len() >= i + 9 && &self.buf[i..i + 9] == b"<![CDATA[" {
                        if let Some(end_rel) = find_sequence(&self.buf[i..self.len], b"]]>") {
                            let end = i + end_rel + 3;
                            out.extend_from_slice(&self.buf[i..end]);
                            read_pos = end;
                            continue;
                        } else {
                            self.refill()?;
                            continue;
                        }
                    } else {
                        // other declaration: copy until next '>'
                        if let Some(gt_rel) = memchr::memchr(b'>', &self.buf[i..self.len]) {
                            let gt = i + gt_rel;
                            out.extend_from_slice(&self.buf[i..gt + 1]);
                            read_pos = gt + 1;
                            continue;
                        } else {
                            self.refill()?;
                            continue;
                        }
                    }
                } else if next == b'?' {
                    // PI: copy until ?>
                    if let Some(end_rel) = find_sequence(&self.buf[i..self.len], b"?>") {
                        let end = i + end_rel + 2;
                        out.extend_from_slice(&self.buf[i..end]);
                        read_pos = end;
                        continue;
                    } else {
                        self.refill()?;
                        continue;
                    }
                } else {
                    // start tag: parse name
                    let rest = &self.buf[i + 1..self.len];
                    let (nlen, _delim) = scan_name(rest, name.len());
                    if i + 1 + nlen > self.len {
                        self.refill()?;
                        if i + 1 + nlen > self.len {
                            return Ok(None);
                        }
                    }
                    let start_name = self.buf[i + 1..i + 1 + nlen].to_vec();
                    // parse attributes to find end of start-tag
                    let after_name = &self.buf[i + 1 + nlen..self.len];
                    let (_parsed_attrs, consumed_attrs, _hit, _ntr, _vtr) =
                        parse_attributes(after_name, 512, 1024, 1024);
                    // ensure we have consumed_attrs bytes available
                    if i + 1 + nlen + consumed_attrs > self.len {
                        self.refill()?;
                        if i + 1 + nlen + consumed_attrs > self.len {
                            return Ok(None);
                        }
                    }
                    let tag_end = i + 1 + nlen + consumed_attrs;
                    // copy bytes
                    out.extend_from_slice(&self.buf[i..tag_end]);
                    read_pos = tag_end;
                    // if this is the starting element name, increment depth
                    if start_name.as_slice() == name {
                        depth += 1;
                    }
                    // if self-closing (ends with '/>') then adjust depth immediately
                    if tag_end >= 2 && self.buf[tag_end - 2..tag_end].ends_with(b"/>") {
                        if start_name.as_slice() == name {
                            depth -= 1;
                            if depth == 0 {
                                // advance scanner position to read_pos
                                self.pos = read_pos;
                                return Ok(Some(out));
                            }
                        }
                    }
                    continue;
                }
            } else {
                // no '<' found in slice => copy all and refill
                out.extend_from_slice(&self.buf[read_pos..self.len]);
                read_pos = self.len;
                self.refill()?;
                if read_pos >= self.len {
                    // EOF
                    return Ok(None);
                }
            }
        }
    }
}

/// High-level FilteringParser that exposes a single method to find the next
/// element matching the given name and return its complete bytes.
pub struct FilteringParser<R: Read> {
    scanner: FilteringScanner<R>,
    name: Vec<u8>,
}

impl<R: Read> FilteringParser<R> {
    pub fn new(reader: R, name: &str) -> Self {
        FilteringParser {
            scanner: FilteringScanner::new(reader),
            name: name.as_bytes().to_vec(),
        }
    }

    /// Find next matching element and return its full byte sequence (including start and end tags).
    pub fn next_match(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            match self.scanner.find_start_tag(&self.name)? {
                Some(off) => {
                    // ensure the '<' at off is at or after scanner.pos; set read start
                    if let Some(el) = self.scanner.extract_full_element(off, &self.name)? {
                        return Ok(Some(el));
                    } else {
                        return Ok(None);
                    }
                }
                None => return Ok(None),
            }
        }
    }
}
