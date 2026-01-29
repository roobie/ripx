pub mod parser;

// Phase 1: Parallel processing modules
pub mod chunk;
pub mod parallel;
pub mod slice_parser;

// Phase 2: Zero-copy and SIMD optimizations
pub mod arena_parser;
pub mod event;
pub mod simd_scanner;

// Core parser internals (kept private; tests may access via `pub(crate)` as needed)
mod attributes;
mod element_stack;
mod input_buffer;
mod scratch;
mod tokenizer;
