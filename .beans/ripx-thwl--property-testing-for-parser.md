---
# ripx-thwl
title: Property testing for parser
status: todo
type: milestone
priority: normal
created_at: 2026-01-27T12:27:22Z
updated_at: 2026-01-27T12:45:00Z
---

Add property-based tests for the parser to validate parsing correctness over wide inputs.

## Checklist
- [ ] Design properties to check (round-trip, idempotence, error handling)
- [ ] Implement generators for valid and invalid inputs
- [ ] Integrate tests into CI
- [ ] Review and iterate based on failures

## Fault-tolerance test checklist (concrete cases)
Add the following unit tests to exercise parser recovery, buffer-boundary behavior, and robustness.

- [ ] T01: EOF inside element name — input: `<root><brok` — assert: parser returns Err on name read or advance() and no panic; outer `EndElement` preserves accumulator where possible.
- [ ] T02: EOF inside attribute name — input: `<root><tag a` — assert: attribute/name read error surfaced; advance() can resync; no panic.
- [ ] T03: EOF inside attribute value (unterminated quote) — input: `<root><tag a="value` — assert: UnexpectedEof from attribute parsing; accumulator preserved; no panic.
- [ ] T04: EOF inside comment (partial `--`) — input: `<root><!-- incomplete --` — assert: read_until_bytes returns collected bytes and EOF without panic; predictable handling at EOF.
- [ ] T05: EOF inside CDATA (partial `]]>`) — input: `<root><![CDATA[incomplete` — assert: read_until_bytes returns collected CDATA bytes on EOF; no panic.
- [ ] T06: Pattern split across buffer boundary (`-->`) — input: long comment with `-->` bytes split across fill_buf() boundary — assert: Comment event emitted with full content.
- [ ] T07: Pattern split across buffer boundary (`]]>`) — input: CDATA spanning buffer refill — assert: CData event emitted with full content.
- [ ] T08: `try_consume` fails at buffer boundary — input: pattern prefix at the end of a buffer — assert: try_consume returns false and parser falls back to conservative handling; no misconsumption.
- [ ] T09: Self-closing with intervening spaces and attributes — input: `<root><a  attr='x'   /   ></root>` — assert: StartElement then synthetic EndElement; names/attrs correct.
- [ ] T10: Stray `<` not a start tag resynchronizes — input: `<root>text < notatag <child></child></root>` — assert: parser skips stray `<` and later child parsed; no panic.
- [ ] T11: Very long name exceeding internal buffer — input: `<root><` + long name (>>8192 bytes) + `></root>` — assert: read_name accumulates long name without panic and parser handles growth.
- [ ] T12: Attribute containing entity-like bytes left intact — input: `<root a='&amp;&lt;ok'></root>` — assert: attribute value preserved literally (no decoding).
- [ ] T13: Overlapping partial pattern occurrences — input: sequences like `<!-- - - - -->--></root>` — assert: comment termination handled correctly; no stuck loop.
- [ ] T14: Nested broken constructs and coarse recovery — input: `<root><a><b broken<tag/></root>` — assert: no panic; outer Start/End still emitted after recovery.
- [ ] T15: Processing instruction truncated and across boundary — input: `<?pi incomplete` or split across buffers — assert: truncated PI does not panic and parser recovers or finishes cleanly.
- [ ] T16: IO error from underlying reader mid-parse — simulate BufRead returning Err after N bytes — assert: parser surfaces IO error and does not panic.
- [ ] T17: Accumulator preservation after recovery — input: `<root><broken a="</root>` — assert: EndElement.accumulated for `root` includes broken region; accumulator not lost after advance().
- [ ] T18: Arbitrary bytes (fuzz input) do not panic — input: random bytes 0..=255 — assert: parser loop returns events or io::Errors but never panics or loops indefinitely.

## Property-based test ideas
These should be implemented with a property framework (`proptest` recommended) or via a fuzzing harness for the arbitrary-bytes property.

- [ ] `no-panic-on-any-bytes`: For any byte sequence, running the parser loop (calling `next_event` repeatedly and using `advance()` on Err) must not panic or hang; outcome is a finite sequence of events and/or io::Errors.
- [ ] `well-nested-start-end-for-well-formed-input`: For generated well-formed trees (elements, attributes, text, comments, CDATA within parser-supported subset), Start/End events are properly nested; self-closing tags produce Start then synthetic End.
- [ ] `roundtrip-serialization-for-supported-subset`: Serialize generated event trees to bytes with a deterministic writer and reparse; the resulting event sequence should equal the original (within supported features; avoid entities requiring decoding).
- [ ] `pattern-boundary-resilience`: For valid inputs split into arbitrary chunk sizes (simulate BufRead chunking), multi-byte terminators (`-->`, `]]>`, `?>`) must still be recognized and produce identical events.
- [ ] `accumulator-contains-raw-markup`: When `EndElement` is emitted, its `accumulated` contains the raw bytes for the element's markup region where possible.

## Next steps / implementation notes
- Implement test helpers: a `ChunkedReader` mock `BufRead` that yields controlled chunk sizes, and a `FailingReader` that returns an IO error after N bytes.
- Prioritize implementing T03, T04, T06, T07, T09, T11, and T16 as unit tests.
- Integrate `proptest` and/or `cargo-fuzz` for properties and T18.
- Update this bean with results and mark completed items as tests are added and pass.

