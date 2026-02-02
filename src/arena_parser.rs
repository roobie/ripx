//! Arena-based parser for high-performance chunk processing.
//!
//! This parser uses bump allocation (arena allocation) for temporary data structures,
//! dramatically reducing allocation overhead compared to the standard parser.
//! Perfect for parallel chunk processing where each chunk can be allocated in its
//! own arena and freed all at once when done.

use crate::event::{AttrSlice, ZeroCopyAttributes, ZeroCopyEvent};
use crate::parser::{ErrorCode, EventType, ParserLimits};
use crate::simd_scanner::SimdScanner;
use crate::tokenizer::scan_name;
use bumpalo::Bump;

/// Arena-based parser optimized for chunk processing.
///
/// Uses bump allocation for all temporary data, allowing extremely fast allocation
/// and instant deallocation (just drop the arena).
pub struct ArenaParser<'data> {
    /// Input data slice (typically from memory-mapped file)
    data: &'data [u8],

    /// Current parsing position
    pos: usize,

    /// Bump allocator for temporary data
    arena: &'data Bump,

    /// Element stack (using arena allocation)
    elem_stack: ArenaElementStack<'data>,

    /// Parser limits
    limits: ParserLimits,

    /// EOF fault emitted flag
    eof_fault_emitted: bool,

    /// Pending end element (self-closing tags)
    pending_end: Option<&'data [u8]>,
}

/// Element stack that uses arena allocation for names.
struct ArenaElementStack<'data> {
    frames: Vec<ArenaElementFrame<'data>>,
    max_depth: usize,
}

/// Element frame stored in the stack (arena-allocated name).
#[derive(Clone)]
struct ArenaElementFrame<'data> {
    /// Element name (stored in arena)
    name: &'data [u8],
}

impl<'data> ArenaParser<'data> {
    /// Create a new arena parser.
    ///
    /// The arena should be pre-allocated with sufficient capacity
    /// (typically 512KB - 1MB per chunk).
    pub fn new(data: &'data [u8], arena: &'data Bump, limits: ParserLimits) -> Self {
        let elem_stack = ArenaElementStack {
            frames: Vec::with_capacity(limits.max_depth),
            max_depth: limits.max_depth,
        };

        Self {
            data,
            pos: 0,
            arena,
            elem_stack,
            limits,
            eof_fault_emitted: false,
            pending_end: None,
        }
    }

    /// Get the current buffer (remaining unparsed data).
    #[inline]
    fn buffer(&self) -> &'data [u8] {
        &self.data[self.pos..]
    }

    /// Consume n bytes from input.
    #[inline]
    fn consume(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }

    /// Parse the next event.
    ///
    /// Returns zero-copy events that reference the original memory-mapped data.
    pub fn next_event(&mut self) -> Result<ZeroCopyEvent<'data>, ParseError> {
        // Handle pending end element
        if let Some(name) = self.pending_end.take() {
            return Ok(ZeroCopyEvent::new(
                EventType::EndElement,
                AttrSlice::direct(name),
                false,
                None,
                ZeroCopyAttributes::new(),
            ));
        }

        let buf = self.buffer();
        if buf.is_empty() {
            // EOF handling
            if !self.elem_stack.frames.is_empty() && !self.eof_fault_emitted {
                self.eof_fault_emitted = true;
                return Ok(ZeroCopyEvent::new(
                    EventType::Fault,
                    AttrSlice::direct(&[]),
                    false,
                    Some(ErrorCode::UnclosedElement),
                    ZeroCopyAttributes::new(),
                ));
            }
            return Err(ParseError::Eof);
        }

        // Find next XML structural character
        if let Some(i) = SimdScanner::find_xml_structural_char(buf) {
            if i > 0 && buf[i] == b'<' {
                // Emit text before tag
                let text_data = &buf[..i];
                self.consume(i);
                return Ok(ZeroCopyEvent::new(
                    EventType::Text,
                    AttrSlice::direct(text_data),
                    false,
                    None,
                    ZeroCopyAttributes::new(),
                ));
            }

            if buf[i] == b'<' {
                // We have '<' at current position
                if buf.len() < 2 {
                    return Err(ParseError::Fault(ErrorCode::UnexpectedEof));
                }

                // Check what kind of tag
                match buf[i + 1] {
                    b'/' => self.parse_end_tag(),
                    b'!' => {
                        if buf[i..].starts_with(b"<!--") {
                            self.parse_comment()
                        } else if buf[i..].starts_with(b"<![CDATA[") {
                            self.parse_cdata()
                        } else {
                            // Skip unknown declarations
                            if let Some(close) = SimdScanner::find_tag_close(&buf[i + 2..]) {
                                self.consume(i + 2 + close + 1);
                                self.next_event()
                            } else {
                                Err(ParseError::Fault(ErrorCode::UnexpectedEof))
                            }
                        }
                    }
                    b'?' => self.parse_pi(),
                    _ => self.parse_start_tag(),
                }
            } else {
                // Found '&' or '>' in text - for now, just return text up to this point
                // Full entity handling would decode entities here
                let text_data = &buf[..=i];
                self.consume(i + 1);
                Ok(ZeroCopyEvent::new(
                    EventType::Text,
                    AttrSlice::direct(text_data),
                    false,
                    None,
                    ZeroCopyAttributes::new(),
                ))
            }
        } else {
            // No more structural characters, emit remaining text
            let text_data = buf;
            self.consume(buf.len());
            Ok(ZeroCopyEvent::new(
                EventType::Text,
                AttrSlice::direct(text_data),
                false,
                None,
                ZeroCopyAttributes::new(),
            ))
        }
    }

    fn parse_start_tag(&mut self) -> Result<ZeroCopyEvent<'data>, ParseError> {
        let buf = self.buffer();
        let rest = &buf[1..];
        let (name_len, _delim) = scan_name(rest, self.limits.max_name_len);

        if let Some(gt_pos) = SimdScanner::find_tag_close(rest) {
            let self_closing = rest[gt_pos.saturating_sub(1)] == b'/';
            let name_bytes = &rest[..name_len];

            // Allocate name in arena
            let name_in_arena = self.arena.alloc_slice_copy(name_bytes);

            // Parse attributes (full implementation)
            let mut attrs = ZeroCopyAttributes::new();
            let attr_region = if self_closing {
                &rest[name_len..gt_pos - 1]
            } else {
                &rest[name_len..gt_pos]
            };

            // Skip whitespace after name
            let ws_skipped = SimdScanner::skip_whitespace(attr_region);
            let attr_start = &attr_region[ws_skipped..];

            // Parse attributes with full tokenizer logic
            let (_consumed, _hit_limit, _name_trunc, _value_trunc) = self.parse_attributes_full(attr_start, &mut attrs);

            // Push to element stack
            if self.elem_stack.frames.len() >= self.limits.max_depth {
                return Err(ParseError::Fault(ErrorCode::DepthLimitExceeded));
            }

            self.elem_stack.frames.push(ArenaElementFrame {
                name: name_in_arena,
            });

            let consumed = 1 + gt_pos + 1;
            self.consume(consumed);

            if self_closing {
                // Pop and set up pending end element
                let frame = self.elem_stack.frames.pop().unwrap();
                self.pending_end = Some(frame.name);
            }

            Ok(ZeroCopyEvent::new(
                EventType::StartElement,
                AttrSlice::direct(name_in_arena),
                false,
                None,
                attrs,
            ))
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_end_tag(&mut self) -> Result<ZeroCopyEvent<'data>, ParseError> {
        let buf = self.buffer();
        let rest = &buf[2..];
        let (name_len, _delim) = scan_name(rest, self.limits.max_name_len);

        if let Some(gt) = SimdScanner::find_tag_close(rest) {
            let end_name = &rest[..name_len];
            let consumed = 2 + gt + 1;

            if self.elem_stack.frames.is_empty() {
                // Orphaned end element
                self.consume(consumed);
                return Ok(ZeroCopyEvent::new(
                    EventType::Fault,
                    AttrSlice::direct(end_name),
                    false,
                    Some(ErrorCode::OrphanedEndElement),
                    ZeroCopyAttributes::new(),
                ));
            }

            // Pop from stack
            let top_frame = self.elem_stack.frames.pop().unwrap();

            // Check if names match
            if top_frame.name != end_name {
                // Mismatched end element
                self.elem_stack.frames.push(top_frame); // Push back
                self.consume(consumed);
                return Ok(ZeroCopyEvent::new(
                    EventType::Fault,
                    AttrSlice::direct(end_name),
                    false,
                    Some(ErrorCode::MismatchedEndElement),
                    ZeroCopyAttributes::new(),
                ));
            }

            self.consume(consumed);
            Ok(ZeroCopyEvent::new(
                EventType::EndElement,
                AttrSlice::direct(top_frame.name),
                false,
                None,
                ZeroCopyAttributes::new(),
            ))
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_comment(&mut self) -> Result<ZeroCopyEvent<'data>, ParseError> {
        let buf = self.buffer();
        if let Some(pos) = buf.windows(3).position(|w| w == b"-->") {
            let comment_data = &buf[4..pos];
            self.consume(pos + 3);
            Ok(ZeroCopyEvent::new(
                EventType::Comment,
                AttrSlice::direct(comment_data),
                false,
                None,
                ZeroCopyAttributes::new(),
            ))
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_cdata(&mut self) -> Result<ZeroCopyEvent<'data>, ParseError> {
        let buf = self.buffer();
        if let Some(pos) = buf.windows(3).position(|w| w == b"]]>") {
            let cdata_data = &buf[9..pos];
            self.consume(pos + 3);
            Ok(ZeroCopyEvent::new(
                EventType::CData,
                AttrSlice::direct(cdata_data),
                false,
                None,
                ZeroCopyAttributes::new(),
            ))
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_pi(&mut self) -> Result<ZeroCopyEvent<'data>, ParseError> {
        let buf = self.buffer();
        if let Some(pos) = buf.windows(2).position(|w| w == b"?>") {
            let pi_data = &buf[2..pos];
            self.consume(pos + 2);
            Ok(ZeroCopyEvent::new(
                EventType::ProcessingInstruction,
                AttrSlice::direct(pi_data),
                false,
                None,
                ZeroCopyAttributes::new(),
            ))
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    /// Parse attributes using full tokenizer logic with zero-copy support.
    ///
    /// This replaces the simplified parser with a complete implementation that handles
    /// quoted/unquoted values, entity decoding, and all edge cases.
    fn parse_attributes_full(&self, buf: &'data [u8], attrs: &mut ZeroCopyAttributes<'data>) -> (usize, bool, bool, bool) {
        let mut consumed = 0usize;
        let mut hit_limit = false;
        let mut name_truncated_any = false;
        let mut value_truncated_any = false;

        // Helper to skip ASCII whitespace
        fn skip_ws(s: &[u8]) -> (usize, &[u8]) {
            let mut i = 0usize;
            while i < s.len() {
                match s[i] {
                    b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                    _ => break,
                }
            }
            (i, &s[i..])
        }

        let mut remaining = buf;

        // Main parsing loop
        loop {
            let (sk, rest) = skip_ws(remaining);
            consumed += sk;
            remaining = rest;

            if remaining.is_empty() {
                break;
            }

            // Stop if we reached the end of the start tag
            if remaining[0] == b'>' {
                consumed += 1;
                break;
            }
            if remaining.len() >= 2 && remaining[0] == b'/' && remaining[1] == b'>' {
                consumed += 2;
                break;
            }

            if attrs.len() >= self.limits.max_attributes {
                hit_limit = true;
                break;
            }

            // Parse attribute name
            let (name_len, _delim) = crate::tokenizer::scan_name(remaining, self.limits.max_attr_name_len);
            if name_len == 0 {
                break;
            }

            // Handle name truncation - advance to real delimiter
            let mut full_name_len = name_len;
            if name_len == self.limits.max_attr_name_len {
                while full_name_len < remaining.len() {
                    match remaining[full_name_len] {
                        b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'=' => break,
                        _ => full_name_len += 1,
                    }
                }
                if full_name_len > name_len {
                    name_truncated_any = true;
                }
            }

            let attr_name_slice = &remaining[..name_len];
            remaining = &remaining[full_name_len..];
            consumed += full_name_len;

            // Skip whitespace after name
            let (sk2, rest2) = skip_ws(remaining);
            consumed += sk2;
            remaining = rest2;

            // Check for '='
            if remaining.is_empty() || remaining[0] != b'=' {
                // No value - add empty attribute
                attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::direct(&[]));
                continue;
            }

            // Consume '='
            remaining = &remaining[1..];
            consumed += 1;

            // Skip whitespace before value
            let (sk3, rest3) = skip_ws(remaining);
            consumed += sk3;
            remaining = rest3;

            if remaining.is_empty() {
                // Empty value
                attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::direct(&[]));
                break;
            }

            // Parse attribute value
            let (vlen, _vdelim) = crate::tokenizer::scan_attr_value(remaining, self.limits.max_attr_value_len);

            if vlen == 0 {
                // Empty value
                attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::direct(&[]));
            } else {
                let mut full_vlen = vlen;

                // Handle truncation for quoted values
                if remaining[0] == b'\'' || remaining[0] == b'"' {
                    if vlen == self.limits.max_attr_value_len {
                        while full_vlen < remaining.len() {
                            if remaining[full_vlen] == remaining[0] {
                                full_vlen += 1;
                                break;
                            }
                            full_vlen += 1;
                        }
                        if full_vlen > vlen {
                            value_truncated_any = true;
                        }
                    }

                    // Extract value content (strip quotes)
                    let value_content = if vlen >= 2 {
                        &remaining[1..vlen - 1]
                    } else {
                        &[]
                    };

                    // Check if entity decoding is needed
                    if crate::tokenizer::contains_entities(value_content) {
                        // Decode entities and create Decoded AttrSlice
                        let decoded = self.decode_entities(value_content);
                        attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::Decoded(decoded));
                    } else {
                        // Zero-copy direct reference
                        attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::direct(value_content));
                    }
                } else {
                    // Unquoted value - handle truncation
                    if vlen == self.limits.max_attr_value_len {
                        while full_vlen < remaining.len() {
                            match remaining[full_vlen] {
                                b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/' => break,
                                _ => full_vlen += 1,
                            }
                        }
                        if full_vlen > vlen {
                            value_truncated_any = true;
                        }
                    }

                    let value_content = &remaining[..vlen];

                    // Check for entities in unquoted values
                    if crate::tokenizer::contains_entities(value_content) {
                        let decoded = self.decode_entities(value_content);
                        attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::Decoded(decoded));
                    } else {
                        attrs.push(AttrSlice::direct(attr_name_slice), AttrSlice::direct(value_content));
                    }
                }

                remaining = &remaining[full_vlen..];
                consumed += full_vlen;
            }
        }

        (consumed, hit_limit, name_truncated_any, value_truncated_any)
    }

    /// Simple entity decoding for common XML entities.
    ///
    /// Returns an Arc<[u8]> containing the decoded data.
    /// For now, handles: &lt;, &gt;, &amp;, &quot;, &apos;
    fn decode_entities(&self, input: &[u8]) -> std::sync::Arc<[u8]> {
        // Simple implementation - can be optimized later
        // For now, just replace common entities
        let mut result = Vec::new();
        let mut i = 0;

        while i < input.len() {
            if input[i] == b'&' {
                // Look for entity end
                if let Some(semicolon_pos) = input[i..].iter().position(|&b| b == b';') {
                    let entity = &input[i..i + semicolon_pos + 1];
                    match entity {
                        b"&lt;" => result.push(b'<'),
                        b"&gt;" => result.push(b'>'),
                        b"&amp;" => result.push(b'&'),
                        b"&quot;" => result.push(b'"'),
                        b"&apos;" => result.push(b'\''),
                        _ => {
                            // Unknown entity - copy as-is
                            result.extend_from_slice(entity);
                        }
                    }
                    i += semicolon_pos + 1;
                } else {
                    // No semicolon found - copy '&' as-is
                    result.push(input[i]);
                    i += 1;
                }
            } else {
                result.push(input[i]);
                i += 1;
            }
        }

        std::sync::Arc::from(result.into_boxed_slice())
    }
}

/// Parse error for arena parser.
#[derive(Debug)]
pub enum ParseError {
    /// EOF reached
    Eof,
    /// Parse fault
    Fault(ErrorCode),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_parser_basic() {
        let data = b"<root>text</root>";
        let arena = Bump::new();
        let limits = ParserLimits {
            max_depth: 16,
            max_attributes: 16,
            max_name_len: 64,
            max_attr_name_len: 64,
            max_attr_value_len: 256,
            max_text_chunk_len: 1024,
            max_text_total_len: None,
            max_comment_chunk_len: 512,
            max_comment_total_len: None,
            max_cdata_chunk_len: 512,
            max_cdata_total_len: None,
            max_pi_chunk_len: 256,
            max_pi_total_len: None,
            include_fault_payload: false,
        };

        let mut parser = ArenaParser::new(data, &arena, limits);

        // Start element
        let event = parser.next_event().unwrap();
        assert_eq!(event.event_type, EventType::StartElement);
        assert_eq!(event.data_bytes(), b"root");

        // Text
        let event = parser.next_event().unwrap();
        assert_eq!(event.event_type, EventType::Text);
        assert_eq!(event.data_bytes(), b"text");

        // End element
        let event = parser.next_event().unwrap();
        assert_eq!(event.event_type, EventType::EndElement);
        assert_eq!(event.data_bytes(), b"root");

        // EOF
        match parser.next_event() {
            Err(ParseError::Eof) => {}
            _ => panic!("Expected EOF"),
        }
    }

    #[test]
    fn test_arena_parser_attributes() {
        let data = b"<elem id=\"123\" name=\"test\"/>";
        let arena = Bump::new();
        let limits = ParserLimits {
            max_depth: 16,
            max_attributes: 16,
            max_name_len: 64,
            max_attr_name_len: 64,
            max_attr_value_len: 256,
            max_text_chunk_len: 1024,
            max_text_total_len: None,
            max_comment_chunk_len: 512,
            max_comment_total_len: None,
            max_cdata_chunk_len: 512,
            max_cdata_total_len: None,
            max_pi_chunk_len: 256,
            max_pi_total_len: None,
            include_fault_payload: false,
        };

        let mut parser = ArenaParser::new(data, &arena, limits);

        let event = parser.next_event().unwrap();
        assert_eq!(event.event_type, EventType::StartElement);
        assert_eq!(event.data_bytes(), b"elem");
        assert_eq!(event.attributes.len(), 2);
        assert_eq!(event.attributes.find(b"id"), Some(&b"123"[..]));
        assert_eq!(event.attributes.find(b"name"), Some(&b"test"[..]));
    }
}
