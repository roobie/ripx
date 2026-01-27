use proptest::prelude::*;
use ripx::parser::{EventType, Parser, ParserLimits};
use std::io::Cursor;

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

// Helper: reconstruct a best-effort byte sequence from events. We consider
// - Text events: use ev.data
// - Comment events: wrap with "<!--" + data + "-->"
// - EndElement: wrap with "</" + data + ">"
// - StartElement: wrap with "<" + data + ">" (attributes ignored)
// - CData: wrap with "<![CDATA[" + data + "]]>")
// - PI: wrap with "<?" + data + "?>"
fn reconstruct_from_events(events: &[ripx::parser::Event<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for ev in events {
        match ev.event_type {
            EventType::Text => out.extend_from_slice(ev.data),
            EventType::Comment => {
                out.extend_from_slice(b"<!--");
                out.extend_from_slice(ev.data);
                out.extend_from_slice(b"-->");
            }
            EventType::EndElement => {
                out.extend_from_slice(b"</");
                out.extend_from_slice(ev.data);
                out.extend_from_slice(b">");
            }
            EventType::StartElement => {
                out.extend_from_slice(b"<");
                out.extend_from_slice(ev.data);
                out.extend_from_slice(b">");
            }
            EventType::CData => {
                out.extend_from_slice(b"<![CDATA[");
                out.extend_from_slice(ev.data);
                out.extend_from_slice(b"]]>");
            }
            EventType::ProcessingInstruction => {
                out.extend_from_slice(b"<?");
                out.extend_from_slice(ev.data);
                out.extend_from_slice(b"?>");
            }
            EventType::Fault | EventType::Eof => {}
        }
    }
    out
}

proptest! {
    #[test]
    fn token_roundtrip(
        bytes in proptest::collection::vec(any::<u8>(), 0..1024),
        max_depth in 1usize..=16,
        max_attributes in 1usize..=16,
        max_name_len in 1usize..=128,
        max_attr_name_len in 1usize..=128,
        max_attr_value_len in 1usize..=512,
        max_text_chunk_len in 1usize..=512,
        max_comment_chunk_len in 1usize..=256,
        max_cdata_chunk_len in 1usize..=256,
        max_pi_chunk_len in 1usize..=128,
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

        // Collect events until EOF or cap; copy only event type and data into owned vectors
        let cap = bytes.len().saturating_mul(4).saturating_add(100).min(10_000);
        let mut events = Vec::new();
        for _ in 0..cap {
            let ev = p.next_event().expect("next_event");
            if ev.event_type == EventType::Eof { break; }
            let data = ev.data.to_vec();
            events.push((ev.event_type, data));
        }

        // Reconstruct and check that reconstructed bytes are a subsequence of original input
        let mut recon = Vec::new();
        let mut i = 0usize;
        while i < events.len() {
            let (et, d) = &events[i];
            match et {
                EventType::Text => recon.extend_from_slice(d),
                EventType::Comment => { recon.extend_from_slice(b"<!--"); recon.extend_from_slice(d); recon.extend_from_slice(b"-->"); }
                EventType::StartElement => {
                    // if next event is EndElement with same data, treat as self-closing
                    if i + 1 < events.len() {
                        let (next_et, next_d) = &events[i+1];
                        if *next_et == EventType::EndElement && next_d == d {
                            recon.extend_from_slice(b"<"); recon.extend_from_slice(d); recon.extend_from_slice(b"/>");
                            i += 2;
                            continue;
                        }
                    }
                    recon.extend_from_slice(b"<"); recon.extend_from_slice(d);
                }
                EventType::EndElement => { recon.extend_from_slice(b"</"); recon.extend_from_slice(d); recon.extend_from_slice(b">"); }
                EventType::CData => { recon.extend_from_slice(b"<![CDATA["); recon.extend_from_slice(d); recon.extend_from_slice(b"]]>" ); }
                EventType::ProcessingInstruction => { recon.extend_from_slice(b"<?"); recon.extend_from_slice(d); recon.extend_from_slice(b"?>"); }
                EventType::Fault | EventType::Eof => {}
            }
            i += 1;
        }
        prop_assert!(is_subsequence(&recon, &bytes), "reconstructed bytes are not a subsequence of original input");
    }
}

// check if 'needle' is a subsequence of 'haystack' (not necessarily contiguous)
fn is_subsequence(needle: &[u8], haystack: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut i = 0usize;
    for &b in haystack {
        if b == needle[i] {
            i += 1;
            if i == needle.len() {
                return true;
            }
        }
    }
    false
}
