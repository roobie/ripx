use crate::attributes::{AttributeTable, Attributes, InternalAttribute, NameBufferKind};
use crate::element_stack::ElementFrame;
use crate::element_stack::ElementStack;
use crate::input_buffer::InputBuffer;
use crate::scratch::ScratchBuffers;
use crate::tokenizer::scan_name;
use std::io;

/// Public parser limits (see ParserV2.md)
#[derive(Debug, Clone)]
pub struct ParserLimits {
    pub max_depth: usize,
    pub max_attributes: usize,
    pub max_name_len: usize,
    pub max_attr_name_len: usize,
    pub max_attr_value_len: usize,
    pub max_text_chunk_len: usize,
    pub max_text_total_len: Option<usize>,
    pub max_comment_chunk_len: usize,
    pub max_comment_total_len: Option<usize>,
    pub max_cdata_chunk_len: usize,
    pub max_cdata_total_len: Option<usize>,
    pub max_pi_chunk_len: usize,
    pub max_pi_total_len: Option<usize>,
    /// Include accumulated bytes in Fault events' `data` field when true.
    pub include_fault_payload: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    StartElement,
    EndElement,
    Text,
    Comment,
    CData,
    ProcessingInstruction,
    Fault,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnexpectedEof,
    UnclosedElement,
    OrphanedEndElement,
    MismatchedQuotes,
    TooManyAttributes,
    DepthLimitExceeded,
    TokenTooLong,
    AttributeNameTooLong,
    AttributeValueTooLong,
    NameTooLong,
    TextTooLong,
    CommentTooLong,
    CDataTooLong,
    ProcessingInstructionTooLong,
    InvalidStructure,
    IoError,
    MismatchedEndElement,
}

pub struct Attribute<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

pub struct Event<'a> {
    pub event_type: EventType,
    pub data: &'a [u8],
    pub is_continuation: bool,
    pub error: Option<ErrorCode>,
    pub attributes: Attributes<'a>,
}

pub struct Parser<R: io::Read> {
    // private fields: reader, buffers, state
    _reader: R,
    _limits: ParserLimits,
    input: InputBuffer,
    scratch: ScratchBuffers,
    elem_stack: ElementStack,
    pending_end: Option<ElementFrame>,
    eof_fault_emitted: bool,
    attr_table: AttributeTable,
}

impl ParserLimits {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_depth == 0 || self.max_attributes == 0 || self.max_name_len == 0 {
            return Err("limits must be > 0 for depth/attributes/name_len");
        }
        Ok(())
    }
}

impl<R: io::Read> Parser<R> {
    pub fn new(reader: R, limits: ParserLimits) -> Self {
        if let Err(e) = limits.validate() {
            panic!("Invalid ParserLimits: {}", e);
        }
        let input = InputBuffer::with_capacity(4096);
        let scratch = ScratchBuffers::with_limits(
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
        Parser {
            _reader: reader,
            _limits: limits,
            input,
            scratch,
            elem_stack,
            pending_end: None,
            eof_fault_emitted: false,
            attr_table,
        }
    }

    pub fn next_event<'a>(&'a mut self) -> io::Result<Event<'a>> {
        // try to fill buffer
        let _ = self.input.fill_from(&mut self._reader)?;
        // If there's a pending synthetic EndElement, emit it first.
        if let Some(frame) = self.pending_end.take() {
            let attrs = Attributes::from_parts(
                self.attr_table.as_slice(),
                self.input.as_slice(),
                &self.scratch,
            );
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
        // This reclaims space from previously emitted events
        self.elem_stack.truncate_scratch(&mut self.scratch);
        let buf = self.input.as_slice();
        if buf.is_empty() {
            // EOF: if there are unclosed elements, emit a single UnclosedElement fault first
            if self.elem_stack.len() > 0 && !self.eof_fault_emitted {
                self.eof_fault_emitted = true;
                let attrs = Attributes::from_parts(
                    self.attr_table.as_slice(),
                    self.input.as_slice(),
                    &self.scratch,
                );
                return Ok(Event {
                    event_type: EventType::Fault,
                    data: &[],
                    is_continuation: false,
                    error: Some(ErrorCode::UnclosedElement),
                    attributes: attrs,
                });
            }
            let attrs = Attributes::from_parts(
                self.attr_table.as_slice(),
                self.input.as_slice(),
                &self.scratch,
            );
            return Ok(Event {
                event_type: EventType::Eof,
                data: &[],
                is_continuation: false,
                error: None,
                attributes: attrs,
            });
        }

        // find next '<'
        if let Some(i) = buf.iter().position(|&b| b == b'<') {
            if i > 0 {
                // emit text before tag
                self.scratch.text.clear();
                let rem = self
                    .scratch
                    .text
                    .capacity()
                    .saturating_sub(self.scratch.text.len());
                let take = i.min(rem);
                self.scratch.text.extend_from_slice(&buf[..take]);
                // consume taken bytes
                self.input.consume(take);
                let attrs = Attributes::from_parts(
                    self.attr_table.as_slice(),
                    self.input.as_slice(),
                    &self.scratch,
                );
                return Ok(Event {
                    event_type: EventType::Text,
                    data: &self.scratch.text[..take],
                    is_continuation: false,
                    error: None,
                    attributes: attrs,
                });
            }
            // buf[0] == '<'
            if buf.starts_with(b"<!--") {
                if let Some(len) = crate::tokenizer::consume_comment(buf) {
                    self.scratch.comment.clear();
                    let inner = &buf[4..len - 3];
                    let _ = self.scratch.push_comment_chunk(inner);
                    self.input.consume(len);
                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.input.as_slice(),
                        &self.scratch,
                    );
                    return Ok(Event {
                        event_type: EventType::Comment,
                        data: &self.scratch.comment[..],
                        is_continuation: false,
                        error: None,
                        attributes: attrs,
                    });
                }
            }
            if buf.len() >= 2 && buf[1] == b'/' {
                // end tag: </name>
                let rest = &buf[2..];
                let (nlen, _delim) = scan_name(rest, self._limits.max_name_len);
                // find closing '>'
                if let Some(gt) = rest.iter().position(|&b| b == b'>') {
                    // Copy the end-tag name into a local buffer so we don't disturb scratch.name
                    let end_name = rest[..nlen].to_vec();
                    let consumed = 2 + gt + 1; // </ + name .. >

                    // If there's no open element to match, return EndElement (preserve prior behavior).
                    if self.elem_stack.len() == 0 {
                        // Append the parsed end-name into scratch.name so we can return a reference
                        let start = self.scratch.name.len();
                        let (copied, _truncated) = self.scratch.push_name(&end_name);
                        let data_slice = &self.scratch.name[start..start + copied];
                        self.input.consume(consumed);
                        let attrs = Attributes::from_parts(
                            self.attr_table.as_slice(),
                            self.input.as_slice(),
                            &self.scratch,
                        );
                        return Ok(Event {
                            event_type: EventType::EndElement,
                            data: data_slice,
                            is_continuation: false,
                            error: None,
                            attributes: attrs,
                        });
                    }

                    // Compare end name to the top of the element stack
                    let top = self.elem_stack.top().unwrap();
                    let start = top.name_start;
                    let end = start + top.name_len;
                    let open_name = &self.scratch.name[start..end];
                    if open_name != end_name.as_slice() {
                        // mismatched end name -> OrphanedEndElement fault
                        let payload: &[u8];
                        if self._limits.include_fault_payload {
                            self.scratch.text.clear();
                            let available = buf.len().min(self.scratch.text.capacity());
                            let _ = self.scratch.push_text_chunk(&buf[..available]);
                            payload = &self.scratch.text[..];
                        } else {
                            payload = &[];
                        }
                        self.input.consume(consumed);
                        loop {
                            let _ = self.input.fill_from(&mut self._reader)?;
                            let b2 = self.input.as_slice();
                            if b2.is_empty() {
                                break;
                            }
                            if let Some(pos) = b2.iter().position(|&c| c == b'<') {
                                if pos > 0 {
                                    self.input.consume(pos);
                                }
                                break;
                            } else {
                                let n = b2.len();
                                self.input.consume(n);
                            }
                        }
                        let attrs = Attributes::from_parts(
                            self.attr_table.as_slice(),
                            self.input.as_slice(),
                            &self.scratch,
                        );
                        return Ok(Event {
                            event_type: EventType::Fault,
                            data: payload,
                            is_continuation: false,
                            error: Some(ErrorCode::MismatchedEndElement),
                            attributes: attrs,
                        });
                    }

                    // Names match: pop and emit EndElement
                    let f = self.elem_stack.pop().unwrap();
                    let emit_frame = ElementFrame {
                        name_start: f.name_start,
                        name_len: f.name_len,
                    };
                    self.input.consume(consumed);
                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.input.as_slice(),
                        &self.scratch,
                    );
                    let start = emit_frame.name_start;
                    let end = start + emit_frame.name_len;
                    return Ok(Event {
                        event_type: EventType::EndElement,
                        data: &self.scratch.name[start..end],
                        is_continuation: false,
                        error: None,
                        attributes: attrs,
                    });
                }
            }
            // start tag: '<name ...>' (not comment, not end, not PI/DOCTYPE)
            if buf.len() >= 2 && buf[1] != b'!' && buf[1] != b'/' && buf[1] != b'?' {
                let rest = &buf[1..];
                let (name_len, _delim) = scan_name(rest, self._limits.max_name_len);
                if name_len == 0 {
                    // malformed; consume '<' and return it as text
                    self.input.consume(1);
                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.input.as_slice(),
                        &self.scratch,
                    );
                    return Ok(Event {
                        event_type: EventType::Text,
                        data: b"<",
                        is_continuation: false,
                        error: None,
                        attributes: attrs,
                    });
                }
                // If truncated by scan_name, advance to actual delimiter so trailing chars don't parse as attrs
                let mut full_name_len = name_len;
                if name_len == self._limits.max_name_len {
                    while full_name_len < rest.len() {
                        match rest[full_name_len] {
                            b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'=' => break,
                            _ => full_name_len += 1,
                        }
                    }
                    // If the actual name extends beyond the configured max, emit NameTooLong fault
                    if full_name_len > name_len {
                        // prepare optional payload
                        let payload: &[u8];
                        if self._limits.include_fault_payload {
                            self.scratch.text.clear();
                            let available = buf.len().min(self.scratch.text.capacity());
                            let _ = self.scratch.push_text_chunk(&buf[..available]);
                            payload = &self.scratch.text[..];
                        } else {
                            payload = &[];
                        }
                        // consume the examined tag bytes ("<" + full name) then skip until next '<' to recover
                        self.input.consume(1 + full_name_len);
                        loop {
                            let _ = self.input.fill_from(&mut self._reader)?;
                            let b2 = self.input.as_slice();
                            if b2.is_empty() {
                                break;
                            }
                            if let Some(pos) = b2.iter().position(|&c| c == b'<') {
                                if pos > 0 {
                                    self.input.consume(pos);
                                }
                                break;
                            } else {
                                let n = b2.len();
                                self.input.consume(n);
                            }
                        }
                        let attrs = Attributes::from_parts(
                            self.attr_table.as_slice(),
                            self.input.as_slice(),
                            &self.scratch,
                        );
                        return Ok(Event {
                            event_type: EventType::Fault,
                            data: payload,
                            is_continuation: false,
                            error: Some(ErrorCode::NameTooLong),
                            attributes: attrs,
                        });
                    }
                }

                // parse attributes from the bytes after the name
                let after_name = &rest[full_name_len..];
                let (parsed_attrs, consumed_attrs, hit_limit, name_trunc, value_trunc) =
                    crate::tokenizer::parse_attributes(
                        after_name,
                        self._limits.max_attr_name_len,
                        self._limits.max_attr_value_len,
                        self._limits.max_attributes,
                    );

                // If attributes parser didn't find a closing '>' or '/>', treat as text fallback.
                // Note: parse_attributes may consume whitespace even when no attributes were parsed,
                // so check parsed_attrs.is_empty() rather than consumed_attrs == 0.
                if parsed_attrs.is_empty() && !after_name.iter().any(|&b| b == b'>') {
                    self.input.consume(1);
                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.input.as_slice(),
                        &self.scratch,
                    );
                    return Ok(Event {
                        event_type: EventType::Text,
                        data: b"<",
                        is_continuation: false,
                        error: None,
                        attributes: attrs,
                    });
                }

                // total bytes to consume for the whole start tag
                let total_consumed = 1 + full_name_len + consumed_attrs;

                // If any attribute name or value was truncated by limits, emit appropriate Fault
                if name_trunc || value_trunc {
                    // choose error code: prefer AttributeNameTooLong if name_trunc
                    let err = if name_trunc {
                        ErrorCode::AttributeNameTooLong
                    } else {
                        ErrorCode::AttributeValueTooLong
                    };
                    let payload: &[u8];
                    if self._limits.include_fault_payload {
                        self.scratch.text.clear();
                        let available = buf.len().min(self.scratch.text.capacity());
                        let _ = self.scratch.push_text_chunk(&buf[..available]);
                        payload = &self.scratch.text[..];
                    } else {
                        payload = &[];
                    }
                    // consume inspected bytes and skip to next '<' to recover
                    self.input.consume(total_consumed);
                    loop {
                        let _ = self.input.fill_from(&mut self._reader)?;
                        let b2 = self.input.as_slice();
                        if b2.is_empty() {
                            break;
                        }
                        if let Some(pos) = b2.iter().position(|&c| c == b'<') {
                            if pos > 0 {
                                self.input.consume(pos);
                            }
                            break;
                        } else {
                            let n = b2.len();
                            self.input.consume(n);
                        }
                    }
                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.input.as_slice(),
                        &self.scratch,
                    );
                    return Ok(Event {
                        event_type: EventType::Fault,
                        data: payload,
                        is_continuation: false,
                        error: Some(err),
                        attributes: attrs,
                    });
                }

                // prepare attribute table and scratch areas
                self.attr_table.clear();
                self.scratch.attr_name.clear();
                self.scratch.attr_value.clear();

                let mut too_many = false;
                for (name_vec, val_vec) in parsed_attrs.into_iter() {
                    if self.attr_table.len() >= self.attr_table.capacity {
                        too_many = true;
                        break;
                    }
                    let name_start = self.scratch.attr_name.len();
                    let (copied_n, _trn) = self.scratch.push_attr_name(&name_vec);
                    let name_len_copied = copied_n;
                    let val_start = self.scratch.attr_value.len();
                    let (copied_v, _trv) = self.scratch.push_attr_value(&val_vec);
                    let val_len_copied = copied_v;
                    let ia = InternalAttribute {
                        name_buffer: NameBufferKind::AttrNameScratch,
                        name_start,
                        name_len: name_len_copied,
                        value_buffer: NameBufferKind::AttrValueScratch,
                        value_start: val_start,
                        value_len: val_len_copied,
                    };
                    let _ = self.attr_table.push_entry(ia);
                }

                // If attribute parser hit limit or we detected overflow, emit Fault
                if hit_limit || too_many {
                    // include payload if requested (copy a prefix of the current buffer)
                    let payload: &[u8];
                    if self._limits.include_fault_payload {
                        self.scratch.text.clear();
                        let available = buf.len().min(self.scratch.text.capacity());
                        let _ = self.scratch.push_text_chunk(&buf[..available]);
                        payload = &self.scratch.text[..];
                    } else {
                        payload = &[];
                    }
                    // consume the start-tag bytes we looked at, then skip forward until next '<' to recover
                    self.input.consume(total_consumed);
                    loop {
                        let _ = self.input.fill_from(&mut self._reader)?;
                        let b2 = self.input.as_slice();
                        if b2.is_empty() {
                            break;
                        }
                        if let Some(pos) = b2.iter().position(|&c| c == b'<') {
                            if pos > 0 {
                                self.input.consume(pos);
                            }
                            break;
                        } else {
                            let n = b2.len();
                            self.input.consume(n);
                        }
                    }
                    let attrs = Attributes::from_parts(
                        self.attr_table.as_slice(),
                        self.input.as_slice(),
                        &self.scratch,
                    );
                    return Ok(Event {
                        event_type: EventType::Fault,
                        data: payload,
                        is_continuation: false,
                        error: Some(ErrorCode::TooManyAttributes),
                        attributes: attrs,
                    });
                }

                // Determine if self-closing by looking at the consumed region
                let tag_region = &buf[1 + full_name_len..1 + full_name_len + consumed_attrs];
                let self_closing = tag_region.ends_with(b"/>");

                // push element name onto stack (copy into scratch.name)
                // use the truncated name portion (name_len)
                let name_bytes = &rest[..name_len];
                match self.elem_stack.push_name(
                    &mut self.scratch,
                    name_bytes,
                    self._limits.max_name_len,
                ) {
                    Ok(()) => {
                        // if self-closing, pop and save pending end
                        let frame = if self_closing {
                            // pop the pushed frame and keep a copy for emission
                            let f = self.elem_stack.pop().unwrap();
                            let emit_frame = ElementFrame {
                                name_start: f.name_start,
                                name_len: f.name_len,
                            };
                            self.pending_end = Some(f);
                            emit_frame
                        } else {
                            // copy top for event emission
                            let top = self.elem_stack.top().unwrap();
                            ElementFrame {
                                name_start: top.name_start,
                                name_len: top.name_len,
                            }
                        };
                        // consume tag bytes
                        self.input.consume(total_consumed);
                        let attrs = Attributes::from_parts(
                            self.attr_table.as_slice(),
                            self.input.as_slice(),
                            &self.scratch,
                        );
                        let start = frame.name_start;
                        let end = start + frame.name_len;
                        return Ok(Event {
                            event_type: EventType::StartElement,
                            data: &self.scratch.name[start..end],
                            is_continuation: false,
                            error: None,
                            attributes: attrs,
                        });
                    }
                    Err(()) => {
                        // depth exceeded
                        self.input.consume(total_consumed);
                        let attrs = Attributes::from_parts(
                            self.attr_table.as_slice(),
                            self.input.as_slice(),
                            &self.scratch,
                        );
                        return Ok(Event {
                            event_type: EventType::Fault,
                            data: &[],
                            is_continuation: false,
                            error: Some(ErrorCode::DepthLimitExceeded),
                            attributes: attrs,
                        });
                    }
                }
            }
            // Start tag or other: for now, treat as text delimiter and consume '<'
            // consume the '<' and return it as text
            self.input.consume(1);
            let attrs = Attributes::from_parts(
                self.attr_table.as_slice(),
                self.input.as_slice(),
                &self.scratch,
            );
            return Ok(Event {
                event_type: EventType::Text,
                data: b"<",
                is_continuation: false,
                error: None,
                attributes: attrs,
            });
        } else {
            // no '<' found, emit all as text
            let rem = self
                .scratch
                .text
                .capacity()
                .saturating_sub(self.scratch.text.len());
            let take = buf.len().min(rem);
            self.scratch.text.clear();
            self.scratch.text.extend_from_slice(&buf[..take]);
            self.input.consume(take);
            let attrs = Attributes::from_parts(
                self.attr_table.as_slice(),
                self.input.as_slice(),
                &self.scratch,
            );
            return Ok(Event {
                event_type: EventType::Text,
                data: &self.scratch.text[..take],
                is_continuation: false,
                error: None,
                attributes: attrs,
            });
        }
    }
}
