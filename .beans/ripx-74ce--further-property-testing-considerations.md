---
# ripx-74ce
title: further property testing considerations
status: draft
type: task
priority: normal
created_at: 2026-01-27T20:12:48Z
updated_at: 2026-01-27T20:13:11Z
---

the top candidates for additional properties to test, prioritized by their value for a streaming, fault-tolerant XML parser:

<hr>

## High-Priority Properties

### 1. **Idempotence of serialization round-trips**
Parse → serialize → parse again should yield identical event streams. This catches encoding issues, whitespace handling bugs, and ensures your parser doesn't lose information.

```rust
fn events_match(tree: &Node, events: Vec<Event>) -> bool {
    // Compare tree structure to event sequence
}
```

### 2. **Monotonic position tracking**
For streaming large files, **byte offsets must never decrease** and should advance predictably. Test that `ev.position` (or similar) is monotonically increasing and that position + data length equals the next position.

```rust
let mut last_pos = 0;
for ev in events {
    prop_assert!(ev.position >= last_pos);
    last_pos = ev.position;
}
```

### 3. **Memory bounds under adversarial input**
Generate **deeply nested** or **wide** trees (1000+ children, 100+ depth) and verify the parser doesn't allocate unbounded memory. Track `parser.memory_used()` or use a custom allocator to assert peak memory stays below `O(max_depth)`.

```rust
// Generate pathological trees
let deep = (0..1000).fold(Node::Text("x".into()), |acc, i| {
    Node::Element(format!("e{}", i), vec![acc])
});
```

### 4. **Fault tolerance: partial parse recovery**
Inject **malformed XML** (unclosed tags, invalid UTF-8, truncated input) and verify:
- Parser emits `EventType::Fault` with meaningful error info
- Parser doesn't panic or hang
- You can resume parsing after a fault (if that's a design goal)

```rust
proptest! {
    #[test]
    fn handles_truncation(tree in arb_node(), cut_at: usize) {
        let mut bytes = Vec::new();
        serialize_node(&tree, &mut bytes);
        bytes.truncate(cut_at);
        
        let mut parser = Parser::new(Cursor::new(bytes), limits);
        let mut fault_seen = false;
        for _ in 0..1000 {
            match parser.next_event() {
                Ok(ev) if ev.event_type == EventType::Fault => {
                    fault_seen = true;
                    break;
                }
                Err(_) => break,
                _ => {}
            }
        }
        // Don't assert fault_seen if cut_at is past valid XML
    }
}
```

<hr>

## Medium-Priority Properties

### 5. **Attribute preservation**
Extend your generator to include attributes and verify they're emitted correctly (order may vary, but key-value pairs must match).

### 6. **Text chunking consistency**
For `max_text_chunk_len`, verify that:
- Text is split at chunk boundaries, not mid-character
- Concatenating all `EventType::Text` chunks reconstructs the original text
- No spurious empty text events

### 7. **Determinism**
Parsing the same input twice should yield **identical event sequences** (same order, same data, same positions). This catches non-deterministic bugs (e.g., hash map iteration order leaking into output).

<hr>

## Lower-Priority (But Still Useful)

### 8. **Performance bounds**
Generate inputs of known size `N` and assert parsing completes in `O(N)` time (not `O(N²)`). Use `#[test]` with `#[ignore]` and `cargo test --release --ignored` for benchmarks.

### 9. **Namespace handling**
If you support namespaces, test that prefixes resolve correctly and don't leak across scopes.

### 10. **Comment/CDATA/PI pass-through**
Generate trees with these node types and verify they're emitted as-is (or skipped, depending on your design).

<hr>

## Implementation Tips

- **Start with #1 (round-trip), #2 (positions), and #4 (fault tolerance)**—they catch the most bugs for streaming parsers.
- Use `proptest`'s **shrinking** to minimize failing cases (it's already doing this, but complex properties benefit from custom `Arbitrary` impls).
- For adversarial inputs (#3, #4), use `prop_filter` to generate only "interesting" malformations (e.g., `bytes.truncate(bytes.len() - 1)` vs random truncation).

These properties will stress **performance, resilience, and edge-case handling**—critical for multi-GB/TB files.
