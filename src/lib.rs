pub mod parser;

// Core parser internals (kept private; tests may access via `pub(crate)` as needed)
mod input_buffer;
mod scratch;
mod element_stack;
mod attributes;
mod tokenizer;
