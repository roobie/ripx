//! Slice-based XML parser for parallel processing.
//!
//! This parser operates on byte slices (`&[u8]`) instead of `io::Read`,
//! enabling zero-copy parsing from memory-mapped files and parallel chunk processing.

use crate::attributes::{AttributeTable, Attributes};
use crate::element_stack::{ElementFrame, ElementStack};
use crate::parser::{ErrorCode, EventType, ParserLimits};
use crate::scratch::ScratchBuffers;
use crate::tokenizer::scan_name;
use memchr;

/// Event returned by the slice parser.
///
/// This is identical to the Event in parser.rs but the lifetime is tied to
/// the slice parser rather than an io::Read stream.
pub struct Event<'a> {
    pub event_type: EventType,
    pub data: &'a [u8],
    pub is_continuation: bool,
    pub error: Option<ErrorCode>,
    pub attributes: Attributes<'a>,
}

/// Error type for slice parsing (no I/O errors possible).
#[derive(Debug)]
pub enum ParseError {
    /// Parse completed (EOF reached)
    Eof,
    /// Parse fault with error code
    Fault(ErrorCode),
}

/// Parser that operates on a byte slice instead of io::Read.
///
/// This parser is designed for parallel processing of XML chunks from
/// memory-mapped files. It maintains the same parsing logic as Parser<R>
/// but eliminates I/O and buffer management overhead.
pub struct SliceParser<'data> {
    /// Input data slice
    input: &'data [u8],
    /// Current position in the input
    pos: usize,
    /// Parser configuration limits
    limits: ParserLimits,
    /// Scratch buffers for token accumulation
    scratch: ScratchBuffers,
    /// Element stack for tracking open elements
    elem_stack: ElementStack,
    /// Attribute table for current element
    attr_table: AttributeTable,
    /// Pending synthetic end element
    pending_end: Option<ElementFrame>,
    /// EOF fault emitted flag
    eof_fault_emitted: bool,
}

impl<'data> SliceParser<'data> {
    /// Create a new slice parser for the given data.
    pub fn new(data: &'data [u8], limits: ParserLimits) -> Self {
        let scratch_capacity = ScratchBuffers::with_limits(
            limits.max_name_len,
            limits.max_attr_name_len,
            limits.max_attr_value_len,
            limits.max_text_chunk_len,
            limits.max_comment_chunk_len,
            limits.max_cdata_chunk_len,
            limits.max_pi_chunk_len,
        );

        let elem_stack = ElementStack::with_capacity(limits.max_depth);
        let attr_table = AttributeTable::with_capacity(limits.max_attributes);

        Self {
            input: data,
            pos: 0,
            limits,
            scratch: scratch_capacity,
            elem_stack,
            attr_table,
            pending_end: None,
            eof_fault_emitted: false,
        }
    }

    /// Get the current buffer (remaining unparsed data).
    #[inline]
    fn buffer(&self) -> &'data [u8] {
        &self.input[self.pos..]
    }

    /// Consume n bytes from the input.
    #[inline]
    fn consume(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.input.len());
    }

    /// Parse the next event from the slice.
    ///
    /// Returns Ok(Event) for each event, or Err(ParseError) when done or on fault.
    pub fn next_event(&mut self) -> Result<Event<'_>, ParseError> {
        // Handle pending end element
        if let Some(frame) = self.pending_end.take() {
            let attrs =
                Attributes::from_parts(self.attr_table.as_slice(), self.buffer(), &self.scratch);
            let start = frame.name_start;
            let end = start + frame.name_len;
            return Ok(Event {
                event_type: EventType::EndElement,
                data: &self.scratch.name[start..end],
                is_continuation: false,
                error: None,
                attributes: attrs,
            });
        }

        // Clean up scratch buffer to match current stack depth
        self.elem_stack.truncate_scratch(&mut self.scratch);

        let buf = self.buffer();
        if buf.is_empty() {
            // EOF handling
            if self.elem_stack.len() > 0 && !self.eof_fault_emitted {
                self.eof_fault_emitted = true;
                let attrs = Attributes::from_parts(self.attr_table.as_slice(), buf, &self.scratch);
                return Ok(Event {
                    event_type: EventType::Fault,
                    data: &[],
                    is_continuation: false,
                    error: Some(ErrorCode::UnclosedElement),
                    attributes: attrs,
                });
            }
            return Err(ParseError::Eof);
        }

        // Find next '<'
        if let Some(i) = memchr::memchr(b'<', buf) {
            if i > 0 {
                // Emit text before tag
                self.scratch.text.clear();
                let rem = self
                    .scratch
                    .text
                    .capacity()
                    .saturating_sub(self.scratch.text.len());
                let take = i.min(rem);
                self.scratch.text.extend_from_slice(&buf[..take]);

                let is_continuation = take < i;
                self.consume(take);

                let attrs = Attributes::from_parts(
                    self.attr_table.as_slice(),
                    self.buffer(),
                    &self.scratch,
                );

                return Ok(Event {
                    event_type: EventType::Text,
                    data: &self.scratch.text[..],
                    is_continuation,
                    error: None,
                    attributes: attrs,
                });
            }

            // We have '<' at position 0
            if buf.len() < 2 {
                return Err(ParseError::Fault(ErrorCode::UnexpectedEof));
            }

            // Check what kind of tag
            match buf[1] {
                b'/' => {
                    // End tag
                    self.parse_end_tag()
                }
                b'!' => {
                    // Comment or CDATA
                    if buf.starts_with(b"<!--") {
                        self.parse_comment()
                    } else if buf.starts_with(b"<![CDATA[") {
                        self.parse_cdata()
                    } else {
                        // Skip unknown declarations (DOCTYPE, etc.)
                        if let Some(close) = memchr::memchr(b'>', &buf[2..]) {
                            self.consume(2 + close + 1);
                            self.next_event()
                        } else {
                            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
                        }
                    }
                }
                b'?' => {
                    // Processing instruction
                    self.parse_pi()
                }
                _ => {
                    // Start tag
                    self.parse_start_tag()
                }
            }
        } else {
            // No more tags, emit remaining text
            self.scratch.text.clear();
            let rem = self.scratch.text.capacity();
            let take = buf.len().min(rem);
            self.scratch.text.extend_from_slice(&buf[..take]);

            let is_continuation = take < buf.len();
            self.consume(take);

            let attrs =
                Attributes::from_parts(self.attr_table.as_slice(), self.buffer(), &self.scratch);

            Ok(Event {
                event_type: EventType::Text,
                data: &self.scratch.text[..],
                is_continuation,
                error: None,
                attributes: attrs,
            })
        }
    }

    fn parse_end_tag(&mut self) -> Result<Event<'_>, ParseError> {
        let buf = self.buffer();
        let rest = &buf[2..];
        let (nlen, _delim) = scan_name(rest, self.limits.max_name_len);

        if let Some(gt) = memchr::memchr(b'>', rest) {
            let end_name = &rest[..nlen];
            let consumed = 2 + gt + 1;

            if self.elem_stack.len() == 0 {
                // Orphaned end element
                let start = self.scratch.name.len();
                let (copied, _truncated) = self.scratch.push_name(end_name);
                self.consume(consumed);

                let attrs = Attributes::from_parts(
                    self.attr_table.as_slice(),
                    self.buffer(),
                    &self.scratch,
                );

                let data_slice = &self.scratch.name[start..start + copied];

                return Ok(Event {
                    event_type: EventType::EndElement,
                    data: data_slice,
                    is_continuation: false,
                    error: None,
                    attributes: attrs,
                });
            }

            // Compare with stack
            let top = self.elem_stack.top().unwrap();
            let start = top.name_start;
            let end = start + top.name_len;
            let open_name = &self.scratch.name[start..end];

            if open_name != end_name {
                // Mismatched end element
                self.consume(consumed);
                let attrs = Attributes::from_parts(
                    self.attr_table.as_slice(),
                    self.buffer(),
                    &self.scratch,
                );

                return Ok(Event {
                    event_type: EventType::Fault,
                    data: end_name,
                    is_continuation: false,
                    error: Some(ErrorCode::MismatchedEndElement),
                    attributes: attrs,
                });
            }

            // Names match: pop and emit
            let f = self.elem_stack.pop().unwrap();
            let emit_frame = ElementFrame {
                name_start: f.name_start,
                name_len: f.name_len,
            };
            self.consume(consumed);

            let attrs =
                Attributes::from_parts(self.attr_table.as_slice(), self.buffer(), &self.scratch);

            let start = emit_frame.name_start;
            let end = start + emit_frame.name_len;

            Ok(Event {
                event_type: EventType::EndElement,
                data: &self.scratch.name[start..end],
                is_continuation: false,
                error: None,
                attributes: attrs,
            })
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_start_tag(&mut self) -> Result<Event<'_>, ParseError> {
        let buf = self.buffer();
        let rest = &buf[1..];
        let (name_len, _delim) = scan_name(rest, self.limits.max_name_len);

        // Find end of tag
        if let Some(gt_pos) = memchr::memchr(b'>', rest) {
            let self_closing = rest[gt_pos.saturating_sub(1)] == b'/';
            let consumed = 1 + gt_pos + 1;

            // Parse attributes (simplified - full implementation would use parse_attributes from tokenizer)
            self.attr_table.clear();

            // Push element name
            let name_bytes = &rest[..name_len];
            match self
                .elem_stack
                .push_name(&mut self.scratch, name_bytes, self.limits.max_name_len)
            {
                Ok(()) => {
                    let frame = if self_closing {
                        let f = self.elem_stack.pop().unwrap();
                        let emit_frame = ElementFrame {
                            name_start: f.name_start,
                            name_len: f.name_len,
                        };
                        self.pending_end = Some(f);
                        emit_frame
                    } else {
                        let top = self.elem_stack.top().unwrap();
                        ElementFrame {
                            name_start: top.name_start,
                            name_len: top.name_len,
                        }
                    };

                    self.consume(consumed);

                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.buffer(),
                        &self.scratch,
                    );

                    let start = frame.name_start;
                    let end = start + frame.name_len;

                    Ok(Event {
                        event_type: EventType::StartElement,
                        data: &self.scratch.name[start..end],
                        is_continuation: false,
                        error: None,
                        attributes: attrs,
                    })
                }
                Err(_) => Err(ParseError::Fault(ErrorCode::DepthLimitExceeded)),
            }
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_comment(&mut self) -> Result<Event<'_>, ParseError> {
        let buf = self.buffer();
        if let Some(end_pos) = crate::tokenizer::find_sequence(buf, b"-->") {
            let content_start = 4; // Skip "<!--"
            let content_end = end_pos;
            let content = &buf[content_start..content_end];

            self.scratch.comment.clear();
            let rem = self.scratch.comment.capacity();
            let take = content.len().min(rem);
            self.scratch.comment.extend_from_slice(&content[..take]);

            let is_continuation = take < content.len();
            self.consume(end_pos + 3); // Skip past "-->"

            let attrs =
                Attributes::from_parts(self.attr_table.as_slice(), self.buffer(), &self.scratch);

            Ok(Event {
                event_type: EventType::Comment,
                data: &self.scratch.comment[..],
                is_continuation,
                error: None,
                attributes: attrs,
            })
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_cdata(&mut self) -> Result<Event<'_>, ParseError> {
        let buf = self.buffer();
        if let Some(end_pos) = crate::tokenizer::find_sequence(buf, b"]]>") {
            let content_start = 9; // Skip "<![CDATA["
            let content_end = end_pos;
            let content = &buf[content_start..content_end];

            self.scratch.cdata.clear();
            let rem = self.scratch.cdata.capacity();
            let take = content.len().min(rem);
            self.scratch.cdata.extend_from_slice(&content[..take]);

            let is_continuation = take < content.len();
            self.consume(end_pos + 3); // Skip past "]]>"

            let attrs =
                Attributes::from_parts(self.attr_table.as_slice(), self.buffer(), &self.scratch);

            Ok(Event {
                event_type: EventType::CData,
                data: &self.scratch.cdata[..],
                is_continuation,
                error: None,
                attributes: attrs,
            })
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }

    fn parse_pi(&mut self) -> Result<Event<'_>, ParseError> {
        let buf = self.buffer();
        if let Some(end_pos) = crate::tokenizer::find_sequence(buf, b"?>") {
            let content_start = 2; // Skip "<?"
            let content_end = end_pos;
            let content = &buf[content_start..content_end];

            self.scratch.pi.clear();
            let rem = self.scratch.pi.capacity();
            let take = content.len().min(rem);
            self.scratch.pi.extend_from_slice(&content[..take]);

            let is_continuation = take < content.len();
            self.consume(end_pos + 2); // Skip past "?>"

            let attrs =
                Attributes::from_parts(self.attr_table.as_slice(), self.buffer(), &self.scratch);

            Ok(Event {
                event_type: EventType::ProcessingInstruction,
                data: &self.scratch.pi[..],
                is_continuation,
                error: None,
                attributes: attrs,
            })
        } else {
            Err(ParseError::Fault(ErrorCode::UnexpectedEof))
        }
    }
}
