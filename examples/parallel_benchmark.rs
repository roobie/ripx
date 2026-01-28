//! Benchmark comparing single-threaded vs parallel XML parsing.

use ripx::chunk::ChunkSplitter;
use ripx::parallel::{ParallelConfig, ParallelProcessor};
use ripx::parser::{Parser, ParserLimits};
use ripx::slice_parser::{ParseError, SliceParser};
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

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
        eprintln!("Example: {} data.xml relation", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let element_name = &args[2];
    let element_name_bytes = element_name.as_bytes();

    println!("Benchmarking XML parsing on: {}", file_path);
    println!("Filtering for element: {}", element_name);
    println!();

    // Get file size
    let metadata = std::fs::metadata(file_path)?;
    let file_size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    println!("File size: {:.2} MB", file_size_mb);
    println!();

    // Test 1: Single-threaded parsing
    println!("=== Single-Threaded Parsing ===");
    let start = Instant::now();
    let single_count = count_elements_single_threaded(file_path, element_name_bytes)?;
    let single_duration = start.elapsed();
    let single_throughput = file_size_mb / single_duration.as_secs_f64();

    println!("Matches found: {}", single_count);
    println!("Time: {:.3}s", single_duration.as_secs_f64());
    println!("Throughput: {:.2} MB/s", single_throughput);
    println!();

    // Test 2: Parallel parsing with default config
    println!("=== Parallel Parsing (Default Config) ===");
    let start = Instant::now();
    let parallel_count = count_elements_parallel(file_path, element_name_bytes)?;
    let parallel_duration = start.elapsed();
    let parallel_throughput = file_size_mb / parallel_duration.as_secs_f64();

    println!("Matches found: {}", parallel_count);
    println!("Time: {:.3}s", parallel_duration.as_secs_f64());
    println!("Throughput: {:.2} MB/s", parallel_throughput);
    println!();

    // Compare results
    println!("=== Comparison ===");
    if single_count == parallel_count {
        println!("✓ Results match!");
    } else {
        println!("✗ Results differ: {} vs {}", single_count, parallel_count);
    }

    let speedup = single_duration.as_secs_f64() / parallel_duration.as_secs_f64();
    println!("Speedup: {:.2}x", speedup);
    println!(
        "Throughput improvement: {:.2} MB/s → {:.2} MB/s ({:.1}% increase)",
        single_throughput,
        parallel_throughput,
        (parallel_throughput / single_throughput - 1.0) * 100.0
    );

    Ok(())
}

fn count_elements_single_threaded(
    file_path: &str,
    element_name: &[u8],
) -> std::io::Result<usize> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut parser = Parser::new(reader, default_limits());

    let mut count = 0;

    loop {
        match parser.next_event() {
            Ok(event) => {
                use ripx::parser::EventType;

                match event.event_type {
                    EventType::StartElement => {
                        if event.data == element_name {
                            count += 1;
                        }
                    }
                    EventType::Eof => break,
                    _ => {}
                }
            }
            Err(_) => break,
        }
    }

    Ok(count)
}

fn count_elements_parallel(file_path: &str, element_name: &[u8]) -> std::io::Result<usize> {
    use memmap2::Mmap;
    use std::path::Path;

    let config = ParallelConfig {
        num_threads: None, // Use all CPUs
        min_chunk_size: 2 * 1024 * 1024,
        max_chunk_size: 16 * 1024 * 1024,
        overlap_bytes: 8 * 1024,
        parser_limits: default_limits(),
    };

    let processor = ParallelProcessor::with_config(config);

    // Memory-map the file
    let file = File::open(file_path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    // Split into chunks
    let splitter = ChunkSplitter::new(
        processor.config().min_chunk_size,
        processor.config().max_chunk_size,
        processor.config().overlap_bytes,
    );
    let chunks = splitter.split(&mmap[..]);

    println!("Split into {} chunks", chunks.len());

    // Process chunks in parallel
    let chunk_counts = processor.process_file(Path::new(file_path), |chunk| {
        let mut parser = SliceParser::new(chunk.data, default_limits());
        let mut count = 0;

        loop {
            match parser.next_event() {
                Ok(event) => {
                    use ripx::parser::EventType;

                    match event.event_type {
                        EventType::StartElement => {
                            if event.data == element_name {
                                count += 1;
                            }
                        }
                        _ => {}
                    }
                }
                Err(ParseError::Eof) => break,
                Err(ParseError::Fault(_)) => break,
            }
        }

        count
    })?;

    // Sum counts from all chunks
    Ok(chunk_counts.iter().sum())
}
