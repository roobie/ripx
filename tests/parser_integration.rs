use ripx::parser::{EventType, Parser, ParserLimits};
use std::io::Cursor;
fn default_limits() -> ripx::parser::ParserLimits {
    ripx::parser::ParserLimits {
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
    }
}

#[test]
fn parser_eof_placeholder() {
    let data = b"";
    let cursor = Cursor::new(data.as_ref());
    let limits = default_limits();
    let mut p = Parser::new(cursor, limits);
    let ev = p.next_event().expect("next_event failed");
    assert_eq!(ev.event_type, EventType::Eof);
}

#[test]
fn parser_1() {
    let data = b"<root></root>";
    let cursor = Cursor::new(data.as_ref());
    let limits = default_limits();
    let mut p = Parser::new(cursor, limits);
    let ev = p.next_event().expect("next_event failed");
    assert_eq!(ev.event_type, EventType::StartElement);
    assert!(ev.data == b"root");
}
