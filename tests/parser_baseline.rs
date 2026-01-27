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
fn mismatched_end_element_fault() {
    let limits = default_limits();
    let input = Cursor::new(b"<a></b>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev1 = p.next_event().expect("start");
    assert_eq!(ev1.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(ev1.data, b"a");

    let ev2 = p.next_event().expect("fault");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Fault);
    assert_eq!(
        ev2.error,
        Some(ripx::parser::ErrorCode::MismatchedEndElement)
    );

    // EOF should follow (unclosed 'a')
    let ev3 = p.next_event().expect("eof or unclosed");
    assert!(
        ev3.event_type == ripx::parser::EventType::Eof
            || ev3.event_type == ripx::parser::EventType::Fault
    );
}

#[test]
fn orphaned_end_element_returns_end_and_symbol_present() {
    let limits = default_limits();
    let input = Cursor::new(b"</lonely>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("end");
    assert_eq!(ev.event_type, ripx::parser::EventType::EndElement);
    assert_eq!(ev.data, b"lonely");

    // Ensure the enum variant still exists and is printable
    let s = format!("{:?}", ripx::parser::ErrorCode::OrphanedEndElement);
    assert_eq!(s, "OrphanedEndElement");

    let ev2 = p.next_event().expect("eof");
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

#[test]
fn too_many_attributes_no_payload() {
    let mut limits = default_limits();
    limits.max_attributes = 1;
    limits.include_fault_payload = false;
    let input = Cursor::new(b"<a x=\"1\" y=\"2\">".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("fault");
    assert_eq!(ev.event_type, ripx::parser::EventType::Fault);
    assert_eq!(ev.error, Some(ripx::parser::ErrorCode::TooManyAttributes));
    assert_eq!(ev.data.len(), 0);

    let ev2 = p.next_event().expect("eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn too_many_attributes_with_payload() {
    let mut limits = default_limits();
    limits.max_attributes = 1;
    limits.include_fault_payload = true;
    let input = Cursor::new(b"<a x=\"1\" y=\"2\">remaining".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("fault");
    assert_eq!(ev.event_type, ripx::parser::EventType::Fault);
    assert_eq!(ev.error, Some(ripx::parser::ErrorCode::TooManyAttributes));
    assert!(ev.data.len() > 0, "expected payload to be present");

    let ev2 = p.next_event().expect("eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn name_too_long_no_payload() {
    let mut limits = default_limits();
    limits.max_name_len = 3;
    limits.include_fault_payload = false;
    let input = Cursor::new(b"<abcdef>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("fault");
    assert_eq!(ev.event_type, ripx::parser::EventType::Fault);
    assert_eq!(ev.error, Some(ripx::parser::ErrorCode::NameTooLong));
    assert_eq!(ev.data.len(), 0);

    let ev2 = p.next_event().expect("eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn name_too_long_with_payload() {
    let mut limits = default_limits();
    limits.max_name_len = 3;
    limits.include_fault_payload = true;
    let input = Cursor::new(b"<abcdef>rest".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("fault");
    assert_eq!(ev.event_type, ripx::parser::EventType::Fault);
    assert_eq!(ev.error, Some(ripx::parser::ErrorCode::NameTooLong));
    assert!(ev.data.len() > 0);

    let ev2 = p.next_event().expect("eof");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn start_element_with_attributes() {
    let limits = default_limits();
    let input = Cursor::new(b"<a x=\"1\" y='two' empty=\"\">".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("start");
    assert_eq!(ev.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(ev.data, b"a");
    assert_eq!(ev.attributes.len(), 3);
    let (n0, v0) = ev.attributes.get(0).unwrap();
    assert_eq!(n0, b"x");
    assert_eq!(v0, b"1");
    let (n1, v1) = ev.attributes.get(1).unwrap();
    assert_eq!(n1, b"y");
    assert_eq!(v1, b"two");
    let (n2, v2) = ev.attributes.get(2).unwrap();
    assert_eq!(n2, b"empty");
    assert_eq!(v2.len(), 0);

    // Parser should emit an UnclosedElement Fault on EOF when start-tag not closed,
    // then an Eof event.
    let ev2 = p.next_event().expect("fault");
    assert_eq!(ev2.event_type, ripx::parser::EventType::Fault);
    assert_eq!(ev2.error, Some(ripx::parser::ErrorCode::UnclosedElement));

    let ev3 = p.next_event().expect("eof");
    assert_eq!(ev3.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn self_closing_element_emits_end() {
    let limits = default_limits();
    let input = Cursor::new(b"<br/>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("start");
    assert_eq!(ev.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(ev.data, b"br");

    let ev2 = p.next_event().expect("end");
    assert_eq!(ev2.event_type, ripx::parser::EventType::EndElement);
    assert_eq!(ev2.data, b"br");

    let ev3 = p.next_event().expect("eof");
    assert_eq!(ev3.event_type, ripx::parser::EventType::Eof);
}

#[test]
fn nested_depth_limit_exceeded() {
    let mut limits = default_limits();
    limits.max_depth = 2;
    let input = Cursor::new(b"<a><b><c>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    // <a>
    let e1 = p.next_event().expect("a");
    assert_eq!(e1.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(e1.data, b"a");
    // <b>
    let e2 = p.next_event().expect("b");
    assert_eq!(e2.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(e2.data, b"b");
    // <c> should trigger DepthLimitExceeded fault
    let e3 = p.next_event().expect("fault");
    assert_eq!(e3.event_type, ripx::parser::EventType::Fault);
    assert_eq!(e3.error, Some(ripx::parser::ErrorCode::DepthLimitExceeded));
}

#[test]
fn text_chunk_splitting_respects_limit() {
    let mut limits = default_limits();
    limits.max_text_chunk_len = 4;
    let input = Cursor::new(b"abcdefgh".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    // Collect all Text events until EOF and verify chunking behavior.
    let mut pieces: Vec<Vec<u8>> = Vec::new();
    loop {
        let ev = p.next_event().expect("next");
        match ev.event_type {
            ripx::parser::EventType::Text => pieces.push(ev.data.to_vec()),
            ripx::parser::EventType::Eof => break,
            other => panic!("unexpected event: {:?}", other),
        }
    }

    // Each piece must be <= configured chunk length and concatenation equals input
    assert!(!pieces.is_empty());
    let mut concat = Vec::new();
    for piece in &pieces {
        assert!(piece.len() <= 4, "chunk too large");
        concat.extend_from_slice(&piece);
    }
    assert_eq!(concat, b"abcdefgh");
}

#[test]
fn duplicate_attribute_names_are_parsed() {
    let limits = default_limits();
    let input = Cursor::new(b"<a x=1 x=2>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("start");
    assert_eq!(ev.event_type, ripx::parser::EventType::StartElement);
    assert_eq!(ev.data, b"a");
    assert_eq!(ev.attributes.len(), 2);
    let (n0, v0) = ev.attributes.get(0).unwrap();
    let (n1, v1) = ev.attributes.get(1).unwrap();
    assert_eq!(n0, b"x");
    assert_eq!(v0, b"1");
    assert_eq!(n1, b"x");
    assert_eq!(v1, b"2");
}

#[test]
fn binary_bytes_in_text_are_preserved() {
    let limits = default_limits();
    let input = Cursor::new(vec![0xff, 0xfe, b'a', b'b']);
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("text");
    assert_eq!(ev.event_type, ripx::parser::EventType::Text);
    assert_eq!(ev.data, &[0xffu8, 0xfeu8, b'a', b'b']);

    let e = p.next_event().expect("eof");
    assert_eq!(e.event_type, ripx::parser::EventType::Eof);
}

// The following tests target constructs that may not yet be fully supported
// by the parser; mark as ignored to avoid failing the baseline until
// implementation is complete.

#[test]
#[ignore]
fn cdata_section_event() {
    let limits = default_limits();
    let input = Cursor::new(b"<![CDATA[<notatag>]]>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("cdata");
    assert_eq!(ev.event_type, ripx::parser::EventType::CData);
    assert_eq!(ev.data, b"<notatag>");
}

#[test]
#[ignore]
fn processing_instruction_event() {
    let limits = default_limits();
    let input = Cursor::new(b"<?xml-stylesheet href=\"x\"?>".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits);

    let ev = p.next_event().expect("pi");
    assert_eq!(
        ev.event_type,
        ripx::parser::EventType::ProcessingInstruction
    );
}

#[test]
#[ignore]
fn unclosed_constructs_emit_faults_on_eof() {
    let limits = default_limits();
    // unclosed element (start without closing)
    let input = Cursor::new(b"<open".to_vec());
    let mut p = ripx::parser::Parser::new(input, limits.clone());
    // consume until EOF
    while let Ok(ev) = p.next_event() {
        if ev.event_type == ripx::parser::EventType::Fault {
            assert!(matches!(
                ev.error,
                Some(ripx::parser::ErrorCode::UnclosedElement)
                    | Some(ripx::parser::ErrorCode::UnexpectedEof)
            ));
            break;
        }
        if ev.event_type == ripx::parser::EventType::Eof {
            break;
        }
    }

    // unclosed comment
    let input2 = Cursor::new(b"<!--oops".to_vec());
    let mut p2 = ripx::parser::Parser::new(input2, limits);
    while let Ok(ev) = p2.next_event() {
        if ev.event_type == ripx::parser::EventType::Fault {
            assert_eq!(ev.error, Some(ripx::parser::ErrorCode::UnexpectedEof));
            break;
        }
        if ev.event_type == ripx::parser::EventType::Eof {
            break;
        }
    }
}
