# ParserV2 Implementation Plan (Bottom‑Up)

This file records the accepted bottom‑up implementation plan for the streaming XML parser described in ParserV2.md.

Decisions applied:
- Truncate long names/attributes when limits are exceeded.
- Emit a synthetic `EndElement` as a separate, subsequent event after a self‑closing `StartElement`.
- `Attributes<'a>` will resolve attribute name/value slices from the input or scratch buffers.
- `max_*_total_len` limits apply to each contiguous region (e.g., a single CDATA section or contiguous text run).
- Inclusion of accumulated bytes in `Fault` events is configurable; default is `false`.
- Tests live under `tests/`; module unit tests remain in `src/` files where appropriate.

Bottom‑up implementation milestones

1. Input buffer primitive (`src/input_buffer.rs`)
   - Fixed-size sliding buffer with `fill()` and `ensure(n)` operations.
   - Guarantee: slices into the input buffer remain valid until the next `next_event()` call.
   - Tests: underflow/refill, cross-boundary slices, EOF handling.

2. Scratch buffers manager (`src/scratch.rs`)
   - Bounded `Vec<u8>` buffers for `name`, `attr_name`, `attr_value`, `text`, `comment`, `cdata`, `pi`.
   - Never grow beyond configured capacity; `.clear()` and reuse.
   - Tests: capacity invariants, truncation behavior.

3. Element stack (`src/element_stack.rs`)
   - Copy element names into scratch buffer, manage `ElementFrame`s, enforce depth limit.
   - Tests: push/pop, depth exceeded, truncated-name comparisons.

4. Attribute table and resolver (`src/attributes.rs`)
   - `InternalAttribute` entries mapping to buffer kinds + offsets; `Attributes<'a>` resolves slices.
   - Eager parsing per start tag, enforce `max_attributes` and skip further attributes on overflow.
   - Tests: attribute resolution, mixed backing buffers, overflow handling.

5. Tokenizer primitives (`src/tokenizer.rs`)
   - Byte-oriented state helpers for each syntactic region, pattern matching for `-->`, `]]>`, `?>`, etc.
   - Chunking logic per `max_*_chunk_len` and contiguous-region total counters per `max_*_total_len`.
   - Tests: each state machine path, chunking, and transitions to `ErrorRecovery`.

6. Parser core (`src/parser.rs`)
   - Public API: `ParserLimits`, `Parser<R>`, `Event<'a>`, `EventType`, `ErrorCode`, `Attribute`, `Attributes<'a>`.
   - `Parser::new(reader, limits)` validates limits and preallocates buffers.
   - `next_event()` drives tokenizer and returns events borrowing slices that remain valid until the next call.
   - Behavior: truncation on overflow, synthetic `EndElement` emission, Fault payload config.
   - Integration tests in `tests/` for chunking, recovery, EOF-with-open-stack, and synthetic end behavior.

Implementation notes
- Place unit tests adjacent to their modules where practical and broader integration tests under `tests/`.
- Document truncation and synthetic `EndElement` behavior in `ParserV2.md` and public API docs.
- Keep the parser single-threaded; `Event<'a>` lifetimes are tied to mutable borrow of the parser.

Open questions already resolved
- Truncation policy: truncate.
- Synthetic EndElement: emit separately after StartElement.
- Fault payload inclusion: configurable (default false).

Next actions
1. Add module skeletons and public API skeletons in `src/` (input buffer, scratch, element stack, attributes, tokenizer, parser).
2. Update `src/lib.rs` to export the parser API.
3. Add test stubs under `tests/`.

---

Saved plan.
