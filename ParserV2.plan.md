# ParserV2 Implementation Plan (Bottom‑Up)

This file records the accepted bottom‑up implementation plan for the streaming XML parser described in ParserV2.md.

Decisions applied:
- Truncate long names/attributes when limits are exceeded.
- Emit a synthetic `EndElement` as a separate, subsequent event after a self‑closing `StartElement`.
- `Attributes<'a>` will resolve attribute name/value slices from the input or scratch buffers.
- `max_*_total_len` limits apply to each contiguous region (e.g., a single CDATA section or contiguous text run).
- Inclusion of accumulated bytes in `Fault` events is configurable; default is `false`.
- Tests live under `tests/`; module unit tests remain in `src/` files where appropriate.

Bottom‑up implementation milestones (status)

1. Input buffer primitive (`src/input_buffer.rs`)
   - [x] Fixed-size sliding buffer with `fill()` and `ensure(n)` operations.
   - [x] Guarantee: slices into the input buffer remain valid until the next `next_event()` call.
   - [x] Tests: underflow/refill, cross-boundary slices, EOF handling.

2. Scratch buffers manager (`src/scratch.rs`)
   - [x] Bounded `Vec<u8>` buffers for `name`, `attr_name`, `attr_value`, `text`, `comment`, `cdata`, `pi`.
   - [x] Never grow beyond configured capacity; `.clear()` and reuse.
   - [x] Tests: capacity invariants, truncation behavior.

3. Element stack (`src/element_stack.rs`)
   - [x] Copy element names into scratch buffer, manage `ElementFrame`s, enforce depth limit.
   - [x] Tests: push/pop, depth exceeded, truncated-name comparisons.

4. Attribute table and resolver (`src/attributes.rs`)
   - [x] `InternalAttribute` entries mapping to buffer kinds + offsets; `Attributes<'a>` resolves slices.
   - [x] Eager parsing per start tag, enforce `max_attributes` and skip further attributes on overflow.
   - [x] Tests: attribute resolution, mixed backing buffers, overflow handling.

5. Tokenizer primitives (`src/tokenizer.rs`)
   - [x] Byte-oriented helpers for sequences (`-->`, `]]>`, `?>`) and attribute helpers implemented.
   - [ ] Full tokenizer-driven chunking for CDATA/PI/comment/text with `max_*_total_len` counters.
   - [ ] Tests: state-machine paths for chunking and transitions to a unified `ErrorRecovery`.

6. Parser core (`src/parser.rs`)
   - [x] Public API skeleton: `ParserLimits`, `Parser<R>`, `Event<'a>`, `EventType`, `ErrorCode`, `Attribute`, `Attributes<'a>`.
   - [x] `Parser::new(reader, limits)` validates limits and preallocates buffers.
   - [x] `next_event()` drives parsing and returns events borrowing slices that remain valid until the next call.
   - [x] Behavior: truncation on overflow, synthetic `EndElement` emission for self-closing tags, Fault payload config support in many paths.
   - [x] End-tag handling improved: compare end-name to stack top, emit `MismatchedEndElement` on mismatch and return `EndElement` for orphaned close when stack empty.
   - [ ] Implement full spec recovery policy per `ParserV2.md` (emit `InvalidStructure`, synthesize EndElements to recover, then emit `OrphanedEndElement` if no match).
   - [ ] Integration tests for CDATA/PI/DOCTYPE, per-construct `*TooLong` faults, and unified `ErrorRecovery` semantics.

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

Recent updates (delta)
- **End-tag validation implemented:** `End-tag name validation` and the `MismatchedEndElement` error were added to the parser core; explicit end-tags now compare against the element stack.
- **Tests expanded:** `tests/parser_baseline.rs` was expanded with coverage for mismatched and orphaned end-tags, attribute robustness, and text-chunking resilience; baseline tests pass locally.
- **Document reconciliation needed:** an explicit action item is added to reconcile and document the intended recovery semantics for `OrphanedEndElement` vs `MismatchedEndElement` (spec vs current behavior).

Priority changes
- **High priority:** move tokenizer chunking (`max_*_total_len`) and unified `ErrorRecovery` to the top of the implementation backlog — these are required to handle large/ill-formed inputs and to implement spec-compliant recovery.
- **Integration tests:** add integration tests for CDATA/PI/DOCTYPE parsing, per-construct `*TooLong` faults, and the recovery behavior that synthesizes EndElements.

Actionable next steps (concrete)
1. Implement tokenizer chunking and per-construct total counters (text/comment/cdata/pi) and unit tests for chunk boundaries and `*TooLong` faults.
2. Implement CDATA and PI parsing with chunking and `UnexpectedEof` semantics; add integration tests.
3. Implement DOCTYPE skipping with bracket-depth handling and `InvalidStructure` emission; add tests.
4. Implement unified `ErrorRecovery` per spec: single-fault-per-episode skipping until next `<`, synthesize EndElements when required, and document the final behavior (including `OrphanedEndElement` vs `MismatchedEndElement`).
5. Run full test suite and CI, update `ParserV2.plan.md` and public docs to reflect final choices.

---

Saved plan.
