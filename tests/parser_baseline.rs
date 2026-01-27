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

#[test]
fn mixed_text_and_tag_split() {
    let limits = default_limits();
    let input = Cursor::new(b"a<b>c".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev1 = p.next_event().expect("text1");
    assert_eq!(ev1.event_type, ripx::parser::EventType::Text);
    assert_eq!(ev1.data, b"a");

    let ev2 = p.next_event().expect("lt");
    assert_eq!(ev2.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(ev2.data, b"b");

    let ev3 = p.next_event().expect("rest");
    assert_eq!(ev3.event_type, ripx::parser::EventType::Text);
    assert_eq!(ev3.data, b"c");

    let ev4 = p.next_event().expect("fault");
    assert_eq!(ev4.event_type, ripx::parser::EventType::Fault);
    assert_eq!(ev4.error, Some(ripx::parser::ErrorCode::UnclosedElement));

    let ev5 = p.next_event().expect("eof");
    assert_eq!(ev5.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn multiple_comments_sequence() {
    let limits = default_limits();
    let input = Cursor::new(b"<!--a--><!--b-->".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let c1 = p.next_event().expect("c1");
    assert_eq!(c1.event_type, ripx::parser::EventType::Comment);
    assert_eq!(c1.data, b"a");

    let c2 = p.next_event().expect("c2");
    assert_eq!(c2.event_type, ripx::parser::EventType::Comment);
    assert_eq!(c2.data, b"b");

    let e = p.next_event().expect("eof");
    assert_eq!(e.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn lone_left_angle() {
    let limits = default_limits();
    let input = Cursor::new(b"<".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("lt");
    assert_eq!(ev.event_type, ripx::parser::EventType::Text);
    assert_eq!(ev.data, b"<");

    let e = p.next_event().expect("eof");
    assert_eq!(e.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn comment_and_text_mixture() {
    let limits = default_limits();
    let input = Cursor::new(b"hi<!--x-->there".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let t1 = p.next_event().expect("hi");
    assert_eq!(t1.event_type, ripx::parser::EventType::Text);
    assert_eq!(t1.data, b"hi");

    let c = p.next_event().expect("c");
    assert_eq!(c.event_type, ripx::parser::EventType::Comment);
    assert_eq!(c.data, b"x");

    let t2 = p.next_event().expect("there");
    assert_eq!(t2.event_type, ripx::parser::EventType::Text);
    assert_eq!(t2.data, b"there");

    let e = p.next_event().expect("eof");
    assert_eq!(e.event_type, ripx::parser::EventType::Eof);
}
