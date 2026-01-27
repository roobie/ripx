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
fn text_and_eof() {
    let limits = default_limits();
    let input = Cursor::new(b"hello world".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("read event");
    assert_eq!(ev.event_type, ripx::parser::EventType::Text);
    assert_eq!(ev.data, b"hello world");

    let ev2 = p.next_event().expect("read eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn comment_event() {
    let limits = default_limits();
    let input = Cursor::new(b"<!-- inner -->".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("read comment");
    assert_eq!(ev.event_type, ripx::parser::EventType::Comment);
    assert_eq!(ev.data, b" inner ");

    let ev2 = p.next_event().expect("read eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn end_element_event() {
    let limits = default_limits();
    let input = Cursor::new(b"</tag>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("read end");
    assert_eq!(ev.event_type, ripx::parser::EventType::EndElement);
    assert_eq!(ev.data, b"tag");

    let ev2 = p.next_event().expect("read eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}
