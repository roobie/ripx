use std::io::Cursor;
use proptest::prelude::*;
use ripx::parser::{Parser, ParserLimits, EventType};

#[derive(Clone, Debug)]
enum Node {
    Element(String, Vec<Node>),
    Text(String),
}

fn serialize_node(node: &Node, out: &mut Vec<u8>) {
    match node {
        Node::Text(s) => out.extend_from_slice(s.as_bytes()),
        Node::Element(name, children) => {
            out.extend_from_slice(b"<");
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b">");
            for c in children {
                serialize_node(c, out);
            }
            out.extend_from_slice(b"</");
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b">");
        }
    }
}

fn arb_node() -> impl Strategy<Value = Node> {
    let name = proptest::collection::vec(proptest::char::range('a', 'z'), 1..7)
        .prop_map(|v| v.into_iter().collect::<String>());
    let text = proptest::collection::vec(proptest::char::range('a', 'z'), 0..20)
        .prop_map(|v| v.into_iter().collect::<String>());
    let leaf = text.prop_map(Node::Text).boxed();
    leaf.prop_recursive(4, 256, 3, move |inner| {
        (name.clone(), proptest::collection::vec(inner, 0..4))
            .prop_map(|(name, children)| Node::Element(name, children))
            .boxed()
    })
}

proptest! {
    #[test]
    fn structural_integrity(tree in arb_node()) {
        let mut bytes = Vec::new();
        serialize_node(&tree, &mut bytes);

        // Use generous limits so generated trees fit comfortably
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
            include_fault_payload: false,
        };

        let mut parser = Parser::new(Cursor::new(bytes.clone()), limits);
        let mut stack: Vec<Vec<u8>> = Vec::new();
        let cap = bytes.len().saturating_mul(4).saturating_add(100).min(10_000);
        let mut seen_eof = false;

        for _ in 0..cap {
            let ev = parser.next_event().expect("next_event");
            match ev.event_type {
                EventType::StartElement => {
                    let name = ev.data.to_vec();
                    prop_assert!(!name.is_empty(), "start element with empty name");
                    stack.push(name);
                }
                EventType::EndElement => {
                    let name = ev.data.to_vec();
                    let top = stack.pop().expect("end element but stack empty");
                    prop_assert_eq!(top, name, "end element name did not match start");
                }
                EventType::Text => {
                    // nothing to check for now
                }
                EventType::Fault => {
                    prop_assert!(false, "unexpected Fault event: {:?} data={:?} bytes={:?}", ev.error, ev.data, bytes);
                }
                EventType::Comment | EventType::CData | EventType::ProcessingInstruction => {
                    prop_assert!(false, "unexpected event type in generated well-formed XML");
                }
                EventType::Eof => {
                    seen_eof = true;
                    break;
                }
            }
        }

        prop_assert!(seen_eof, "did not reach EOF");
        prop_assert!(stack.is_empty(), "element stack not empty at EOF: {:?}", stack);
    }
}
