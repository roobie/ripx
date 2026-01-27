use std::io::Cursor;
use proptest::prelude::*;
use ripx::parser::{Parser, ParserLimits, EventType};

fn build_limits(
    max_depth: usize,
    max_attributes: usize,
    max_name_len: usize,
    max_attr_name_len: usize,
    max_attr_value_len: usize,
    max_text_chunk_len: usize,
    max_comment_chunk_len: usize,
    max_cdata_chunk_len: usize,
    max_pi_chunk_len: usize,
) -> ParserLimits {
    ParserLimits {
        max_depth,
        max_attributes,
        max_name_len,
        max_attr_name_len,
        max_attr_value_len,
        max_text_chunk_len,
        max_text_total_len: None,
        max_comment_chunk_len,
        max_comment_total_len: None,
        max_cdata_chunk_len,
        max_cdata_total_len: None,
        max_pi_chunk_len,
        max_pi_total_len: None,
        include_fault_payload: false,
    }
}

proptest! {
    #[test]
    fn forward_progress(
        bytes in proptest::collection::vec(any::<u8>(), 0..2048),
        max_depth in 1usize..=32,
        max_attributes in 1usize..=32,
        max_name_len in 1usize..=256,
        max_attr_name_len in 1usize..=256,
        max_attr_value_len in 1usize..=1024,
        max_text_chunk_len in 1usize..=1024,
        max_comment_chunk_len in 1usize..=512,
        max_cdata_chunk_len in 1usize..=512,
        max_pi_chunk_len in 1usize..=256,
    ) {
        let limits = build_limits(
            max_depth,
            max_attributes,
            max_name_len,
            max_attr_name_len,
            max_attr_value_len,
            max_text_chunk_len,
            max_comment_chunk_len,
            max_cdata_chunk_len,
            max_pi_chunk_len,
        );

        let mut p = Parser::new(Cursor::new(bytes.clone()), limits);

        let mut seen_eof = false;
        let cap = bytes.len().saturating_mul(4).saturating_add(100).min(10_000);
        for _ in 0..cap {
            let ev = p.next_event().expect("next_event");
            if ev.event_type == EventType::Eof { seen_eof = true; break; }
        }
        prop_assert!(seen_eof, "parser did not reach EOF within iteration cap");
    }
}
