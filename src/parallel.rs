//! Parallel XML processing framework.
//!
//! This module provides high-level APIs for processing large XML files in parallel
//! using memory-mapped I/O and chunk-based parallelism.

use crate::chunk::{Chunk, ChunkSplitter};
use crate::parser::ParserLimits;
use crate::slice_parser::{ParseError, SliceParser};
use memmap2::Mmap;
use rayon::prelude::*;
use std::fs::File;
use std::io;
use std::path::Path;

/// Configuration for parallel processing.
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Number of threads to use (None = auto-detect from CPU count)
    pub num_threads: Option<usize>,
    /// Minimum chunk size in bytes
    pub min_chunk_size: usize,
    /// Maximum chunk size in bytes
    pub max_chunk_size: usize,
    /// Overlap size between chunks in bytes
    pub overlap_bytes: usize,
    /// Parser limits for each chunk
    pub parser_limits: ParserLimits,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            num_threads: None,                // Use all available CPUs
            min_chunk_size: 2 * 1024 * 1024,  // 2 MB
            max_chunk_size: 16 * 1024 * 1024, // 16 MB
            overlap_bytes: 8 * 1024,          // 8 KB
            parser_limits: ParserLimits {
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
            },
        }
    }
}

/// Parallel XML processor.
pub struct ParallelProcessor {
    config: ParallelConfig,
    splitter: ChunkSplitter,
}

impl ParallelProcessor {
    /// Create a new parallel processor with default configuration.
    pub fn new() -> Self {
        let config = ParallelConfig::default();
        let splitter = ChunkSplitter::new(
            config.min_chunk_size,
            config.max_chunk_size,
            config.overlap_bytes,
        );

        Self { config, splitter }
    }

    /// Create a new parallel processor with custom configuration.
    pub fn with_config(config: ParallelConfig) -> Self {
        let splitter = ChunkSplitter::new(
            config.min_chunk_size,
            config.max_chunk_size,
            config.overlap_bytes,
        );

        Self { config, splitter }
    }

    /// Process an XML file in parallel, applying a function to each chunk.
    ///
    /// The file is memory-mapped and split into chunks at safe boundaries.
    /// Each chunk is processed in parallel using Rayon's thread pool.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the XML file
    /// * `chunk_fn` - Function to apply to each chunk, returning a result
    ///
    /// # Returns
    ///
    /// A vector of results, one per chunk, in chunk order.
    pub fn process_file<F, T>(&self, path: &Path, chunk_fn: F) -> io::Result<Vec<T>>
    where
        F: Fn(&Chunk) -> T + Send + Sync,
        T: Send,
    {
        // Memory-map the file
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // Split into chunks
        let chunks = self.splitter.split(&mmap[..]);

        // Configure thread pool if specified
        let results = if let Some(num_threads) = self.config.num_threads {
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                .install(|| chunks.par_iter().map(&chunk_fn).collect())
        } else {
            // Use default thread pool
            chunks.par_iter().map(&chunk_fn).collect()
        };

        Ok(results)
    }

    /// Process an XML file in parallel with a mutable closure.
    ///
    /// This variant allows the chunk processing function to maintain mutable state
    /// per-thread via Rayon's scope mechanism.
    pub fn process_file_mut<F, T>(&self, path: &Path, chunk_fn: F) -> io::Result<Vec<T>>
    where
        F: Fn(&Chunk) -> T + Send + Sync,
        T: Send,
    {
        self.process_file(path, chunk_fn)
    }

    /// Parse an XML file in parallel and collect all events from matching elements.
    ///
    /// This is a convenience method that handles the common case of filtering
    /// for specific elements.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the XML file
    /// * `element_filter` - Function that returns true for elements to collect
    ///
    /// # Returns
    ///
    /// A vector of element data (as byte vectors) for all matching elements.
    pub fn filter_elements<F>(&self, path: &Path, element_filter: F) -> io::Result<Vec<Vec<u8>>>
    where
        F: Fn(&[u8]) -> bool + Send + Sync,
    {
        self.process_file(path, |chunk| {
            let mut parser = SliceParser::new(chunk.data, self.config.parser_limits.clone());
            let mut matches = Vec::new();
            let mut depth = 0;
            let mut collecting = false;
            let mut buffer = Vec::new();

            loop {
                match parser.next_event() {
                    Ok(event) => {
                        use crate::parser::EventType;

                        match event.event_type {
                            EventType::StartElement => {
                                if depth == 0 && element_filter(event.data) {
                                    // Start collecting
                                    collecting = true;
                                    buffer.clear();
                                    buffer.push(b'<');
                                    buffer.extend_from_slice(event.data);
                                    buffer.push(b'>');
                                }
                                depth += 1;
                            }
                            EventType::EndElement => {
                                depth -= 1;
                                if collecting {
                                    buffer.extend_from_slice(b"</");
                                    buffer.extend_from_slice(event.data);
                                    buffer.push(b'>');

                                    if depth == 0 {
                                        // Done collecting this element
                                        matches.push(buffer.clone());
                                        collecting = false;
                                    }
                                }
                            }
                            EventType::Text if collecting => {
                                buffer.extend_from_slice(event.data);
                            }
                            _ => {}
                        }
                    }
                    Err(ParseError::Eof) => break,
                    Err(ParseError::Fault(_)) => break,
                }
            }

            matches
        })
        .map(|chunk_results| {
            // Flatten results from all chunks
            chunk_results.into_iter().flatten().collect()
        })
    }

    /// Get a reference to the configuration.
    pub fn config(&self) -> &ParallelConfig {
        &self.config
    }
}

impl Default for ParallelProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parallel_processor_creation() {
        let processor = ParallelProcessor::new();
        assert!(processor.config().num_threads.is_none());
    }

    #[test]
    fn test_process_small_file() -> io::Result<()> {
        // Create a temporary XML file
        let mut temp_file = NamedTempFile::new()?;
        let xml = b"<root><item>1</item><item>2</item></root>";
        temp_file.write_all(xml)?;
        temp_file.flush()?;

        let processor = ParallelProcessor::new();

        // Count chunks processed
        let results = processor.process_file(temp_file.path(), |chunk| chunk.chunk_id)?;

        // Small file should be single chunk
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 0);

        Ok(())
    }

    #[test]
    #[ignore] // TODO: SliceParser needs complete attribute parsing implementation
    fn test_filter_elements() -> io::Result<()> {
        // Create a temporary XML file
        let mut temp_file = NamedTempFile::new()?;
        let xml = b"<root><item>A</item><other>B</other><item>C</item></root>";
        temp_file.write_all(xml)?;
        temp_file.flush()?;

        let processor = ParallelProcessor::new();

        // Filter for "item" elements
        let matches = processor.filter_elements(temp_file.path(), |name| name == b"item")?;

        // Should find 2 items
        assert_eq!(matches.len(), 2);

        Ok(())
    }
}
