//! Debug tool to see what the parser is finding.

use ripx::parser::ParserLimits;
use ripx::slice_parser::{ParseError, SliceParser};
use std::env;

fn default_limits() -> ParserLimits {
    ParserLimits {
        max_depth: 256,
        max_attributes: 256,
        max_name_len: 256,
        max_attr_name_len: 512,
        max_attr_value_len: 4096,
        max_text_chunk_len: 16384,
        max_text_total_len: None,
        max_comment_chunk_len: 4096,
        max_comment_total_len: None,
        max_cdata_chunk_len: 16384,
        max_cdata_total_len: None,
        max_pi_chunk_len: 1024,
        max_pi_total_len: None,
        include_fault_payload: false,
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <xml-file> <element-name>", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let element_name = &args[2];
    let element_name_bytes = element_name.as_bytes();

    // Read file
    let data = std::fs::read(file_path)?;
    println!("File size: {} bytes", data.len());
    println!("Looking for element: '{}'", element_name);
    println!();

    // Parse and show first 20 elements
    let mut parser = SliceParser::new(&data, default_limits());
    let mut event_count = 0;
    let mut element_count = 0;
    let mut matches = 0;

    loop {
        match parser.next_event() {
            Ok(event) => {
                event_count += 1;

                use ripx::parser::EventType;
                match event.event_type {
                    EventType::StartElement => {
                        element_count += 1;
                        let name_str = String::from_utf8_lossy(event.data);

                        if event.data == element_name_bytes {
                            matches += 1;
                            println!(
                                "✓ MATCH #{}: <{}> (bytes: {:?})",
                                matches, name_str, event.data
                            );
                        } else if element_count <= 20 {
                            println!("  Element #{}: <{}> (bytes: {:?})", element_count, name_str, event.data);
                        }
                    }
                    EventType::Fault => {
                        println!("FAULT: {:?}", event.error);
                    }
                    _ => {}
                }

                if element_count == 20 && matches == 0 {
                    println!("... (showing first 20 elements only)");
                    println!();
                }
            }
            Err(ParseError::Eof) => break,
            Err(ParseError::Fault(e)) => {
                println!("Parse error: {:?}", e);
                break;
            }
        }
    }

    println!();
    println!("Total events: {}", event_count);
    println!("Total elements: {}", element_count);
    println!("Matches for '{}': {}", element_name, matches);

    Ok(())
}
