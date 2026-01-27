pub mod parser;

// Core parser internals (kept private; tests may access via `pub(crate)` as needed)
mod attributes;
mod element_stack;
mod input_buffer;
mod scratch;
mod tokenizer;
