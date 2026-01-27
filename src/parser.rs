use std::io;
use std::io::Read;
use crate::input_buffer::InputBuffer;
use crate::scratch::ScratchBuffers;
use crate::element_stack::ElementStack;
use crate::attributes::{AttributeTable, Attributes};
use crate::tokenizer::{scan_name};

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
            attr_table,
        }
    }

    pub fn next_event<'a>(&'a mut self) -> io::Result<Event<'a>> {
        // try to fill buffer
        let _ = self.input.fill_from(&mut self._reader)?;
        let buf = self.input.as_slice();
        if buf.is_empty() {
            // EOF
            let attrs = Attributes::from_parts(self.attr_table.as_slice(), self.input.as_slice(), &self.scratch);
            return Ok(Event { event_type: EventType::Eof, data: &[], is_continuation: false, error: None, attributes: attrs });
        }

        // find next '<'
        if let Some(i) = buf.iter().position(|&b| b == b'<') {
            if i > 0 {
                // emit text before tag
                self.scratch.text.clear();
                let rem = self.scratch.text.capacity().saturating_sub(self.scratch.text.len());
                let take = i.min(rem);
                self.scratch.text.extend_from_slice(&buf[..take]);
                // consume taken bytes
                self.input.consume(take);
                let attrs = Attributes::from_parts(self.attr_table.as_slice(), self.input.as_slice(), &self.scratch);
                return Ok(Event { event_type: EventType::Text, data: &self.scratch.text[..take], is_continuation: false, error: None, attributes: attrs });
            }
            // buf[0] == '<'
            if buf.starts_with(b"<!--") {
                if let Some(len) = crate::tokenizer::consume_comment(buf) {
                    self.scratch.comment.clear();
                    let inner = &buf[4..len-3];
                    let _ = self.scratch.push_comment_chunk(inner);
                    self.input.consume(len);
                    let attrs = Attributes::from_parts(self.attr_table.as_slice(), self.input.as_slice(), &self.scratch);
                    return Ok(Event { event_type: EventType::Comment, data: &self.scratch.comment[..], is_continuation: false, error: None, attributes: attrs });
                }
            }
            if buf.len() >= 2 && buf[1] == b'/' {
                // end tag: </name>
                let rest = &buf[2..];
                let (nlen, _delim) = scan_name(rest, self._limits.max_name_len);
                // find closing '>'
                if let Some(gt) = rest.iter().position(|&b| b == b'>') {
                    self.scratch.name.clear();
                    let to_copy = nlen.min(self.scratch.remaining_name_capacity());
                    self.scratch.name.extend_from_slice(&rest[..to_copy]);
                    let consumed = 2 + gt + 1; // </ + name .. >
                    self.input.consume(consumed);
                    let attrs = Attributes::from_parts(self.attr_table.as_slice(), self.input.as_slice(), &self.scratch);
                    return Ok(Event { event_type: EventType::EndElement, data: &self.scratch.name[..to_copy], is_continuation: false, error: None, attributes: attrs });
                }
            }
            // Start tag or other: for now, treat as text delimiter and consume '<'
            // consume the '<' and return it as text
            self.input.consume(1);
            let attrs = Attributes::from_parts(self.attr_table.as_slice(), self.input.as_slice(), &self.scratch);
            return Ok(Event { event_type: EventType::Text, data: b"<", is_continuation: false, error: None, attributes: attrs });
        } else {
            // no '<' found, emit all as text
            let rem = self.scratch.text.capacity().saturating_sub(self.scratch.text.len());
            let take = buf.len().min(rem);
            self.scratch.text.clear();
            self.scratch.text.extend_from_slice(&buf[..take]);
            self.input.consume(take);
            let attrs = Attributes::from_parts(self.attr_table.as_slice(), self.input.as_slice(), &self.scratch);
            return Ok(Event { event_type: EventType::Text, data: &self.scratch.text[..take], is_continuation: false, error: None, attributes: attrs });
        }
    }
}
