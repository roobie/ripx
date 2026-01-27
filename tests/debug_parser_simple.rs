use std::io::Cursor;
use ripx::parser::{Parser, ParserLimits};

#[test]
fn debug_simple_element() {
    let bytes = b"<a></a>".to_vec();
    let limits = ParserLimits {
        max_depth: 64,
        max_attributes: 32,
        max_name_len: 64,
        max_attr_name_len: 64,
        max_attr_value_len: 512,
        max_text_chunk_len: 4096,
        max_text_total_len: None,
        max_comment_chunk_len: 1024,
        max_comment_total_len: None,
        max_cdata_chunk_len: 1024,
        max_cdata_total_len: None,
        max_pi_chunk_len: 512,
        max_pi_total_len: None,
        include_fault_payload: true,
    };
    let mut p = Parser::new(Cursor::new(bytes.clone()), limits);
    loop {
        let ev = p.next_event().expect("next");
        println!("EVENT: {:?} error={:?} data={:?}", ev.event_type, ev.error, ev.data);
        if ev.event_type == ripx::parser::EventType::Eof { break; }
    }
}
