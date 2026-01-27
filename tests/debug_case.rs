use std::io::Cursor;
use ripx::parser::{Parser, ParserLimits, EventType, ErrorCode};

#[test]
fn debug_failing_input() {
    let bytes = vec![60u8, 0u8, 9u8, 0u8];
    let limits = ParserLimits {
        max_depth: 1,
        max_attributes: 1,
        max_name_len: 1,
        max_attr_name_len: 1,
        max_attr_value_len: 1,
        max_text_chunk_len: 1,
        max_text_total_len: None,
        max_comment_chunk_len: 1,
        max_comment_total_len: None,
        max_cdata_chunk_len: 1,
        max_cdata_total_len: None,
        max_pi_chunk_len: 1,
        max_pi_total_len: None,
        include_fault_payload: false,
    };
    let mut p = Parser::new(Cursor::new(bytes.clone()), limits);
    for i in 0..10 {
        let ev = p.next_event().expect("next");
        println!("EV {}: {:?} data={:?} err={:?}", i, ev.event_type, ev.data, ev.error);
        if ev.event_type == EventType::Eof { break; }
    }
}
