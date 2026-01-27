# ripx

`ripx` is a small, single-binary, high-performance, non‑validating XML query tool and library focused on streaming single‑pass processing of very large XML files (multi‑GB / TB). It emphasizes low memory use, simplicity, and predictable performance.

## Goals
- Stream XML from any BufRead source; no DOM, no strict validation.
- Support simple, fast queries (e.g. `//item`, `/root/items/item`) and callbacks for matched subtrees.
- Minimal XML features initially: start/end elements, attributes, text, comments, CDATA.
- Robust best‑effort error handling: skip malformed regions and attempt to recover and continue.

## Features (MVP)
- Streaming reader that emits events: StartElement, EndElement, Text, Comment, CData, Eof.
- Simple attribute parsing; parser now emits raw bytes for textual payloads and does not decode entities (consumers must decode).
- Path stack + small query language: Anywhere (`//name`) and Absolute (`/a/b/c`) selectors.
- Designed to be single‑pass and able to handle huge files.

## Quickstart (development)
Build and run tests with Rust/Cargo:
```bash
cargo build --release
cargo test
```

Example CLI usage (when built):
```bash
# Example (hypothetical) invocation: query `large-file.xml` for an element named `item` with an attribute `id` equal to 12345 and stop when found.
ripx large-file.xml --path "//item" --attr-eq-id="12345" --short-circuit
```

## Project layout
See PLAN.md for the implementation plan. Main modules will include:
- src/lib.rs
- src/reader.rs (streaming reader)
- src/tokenizer.rs (byte scanner)
- src/query.rs (selectors + path stack)
- src/main.rs (CLI / entry point for main binary)

## Future features
- Parallel reading of input file
- Indexing to improve performance

## License

> GPLv3
