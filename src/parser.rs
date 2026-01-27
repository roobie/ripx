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
}

pub struct Attribute<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

/// Lightweight attributes view; implemented in `attributes` module.
pub struct Attributes<'a> {
    // Opaque; internal modules will construct this.
    _private: (),
    _phantom: std::marker::PhantomData<&'a ()>,
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
        Parser { _reader: reader, _limits: limits }
    }

    pub fn next_event<'a>(&'a mut self) -> io::Result<Event<'a>> {
        // Actual implementation will be filled in later; return Eof as placeholder.
        let attrs = Attributes { _private: (), _phantom: std::marker::PhantomData };
        Ok(Event { event_type: EventType::Eof, data: &[], is_continuation: false, error: None, attributes: attrs })
    }
}
