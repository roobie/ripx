use ripx::parser::{Event, EventType};

fn default_limits() -> ripx::parser::ParserLimits {
    ripx::parser::ParserLimits {
        max_depth: 16,
        max_attributes: 16,
        max_name_len: 64,
        max_attr_name_len: 512,
        max_attr_value_len: 512,
        max_text_chunk_len: 1024,
        max_text_total_len: None,
        max_comment_chunk_len: 512,
        max_comment_total_len: None,
        max_cdata_chunk_len: 512,
        max_cdata_total_len: None,
        max_pi_chunk_len: 256,
        max_pi_total_len: None,
        include_fault_payload: false,
    }
}

fn main() -> std::io::Result<()> {
    let limits = default_limits();

    let file_name = std::env::args().nth(1).expect("File missing");
    let file = std::fs::File::open(&file_name)?;
    let reader = std::io::BufReader::new(file);

    // just some adhoc testing
    let element_name = std::env::args().nth(2).expect("Element name required");
    let element_name_bytes = element_name.as_bytes();

    let mut match_counter = 0;
    // pp=pull parser
    let mut pp = ripx::parser::Parser::new(reader, limits);

    let mut depth_in_match: usize = 0;
    let mut output_buffer: Vec<u8> = Vec::with_capacity(1024 * 64);
    let mut temp_buf = [0u8; 4096];

    loop {
        let ev = pp.next_event()?;

        match ev.event_type {
            EventType::StartElement => {
                if depth_in_match > 0 {
                    // Inside match: accumulate nested element
                    write_event_to_buffer(&ev, &mut output_buffer, &mut temp_buf);
                    depth_in_match += 1;
                } else if ev.data == element_name_bytes {
                    if let Some((k,v)) = ev.attributes.get(0) {
                        if k == b"id" && v == b"20121624" {
                            // New match found
                            depth_in_match = 1;
                            match_counter += 1;
                            output_buffer.clear();
                            write_event_to_buffer(&ev, &mut output_buffer, &mut temp_buf);
                        }
                    }
                }
            }

            EventType::EndElement => {
                if depth_in_match > 0 {
                    write_event_to_buffer(&ev, &mut output_buffer, &mut temp_buf);
                    depth_in_match -= 1;

                    if depth_in_match == 0 {
                        // Exited top-level match: print accumulated XML
                        println!("{}", String::from_utf8_lossy(&output_buffer));
                        output_buffer.clear();
                    }
                }
            }

            EventType::Text
            | EventType::Comment
            | EventType::CData
            | EventType::ProcessingInstruction => {
                if depth_in_match > 0 {
                    write_event_to_buffer(&ev, &mut output_buffer, &mut temp_buf);
                }
            }

            EventType::Fault => {
                println!("FAULT: {:?} {}", ev.error, String::from_utf8_lossy(ev.data));
            }

            EventType::Eof => {
                break;
            }
        }
    }

    println!("Found {} matches", match_counter);

    Ok(())
}

trait FormattableEvent {
    fn write_start_element(&self, out: &mut [u8]) -> Result<usize, &str>;
}

impl<'a> FormattableEvent for Event<'a> {
    /// formats the event as a start element
    fn write_start_element(&self, out: &mut [u8]) -> Result<usize, &str> {
        match self.event_type {
            EventType::StartElement => {
                let buf_too_small = Err("Buffer is too small");
                let buffer_size = out.len();

                if buffer_size == 0 {
                    return buf_too_small;
                }

                let mut offset = 0;

                // '<'
                out[offset] = b'<';
                offset += 1;

                // element name
                let name_len = self.data.len();
                if buffer_size < offset + name_len {
                    return buf_too_small;
                }
                out[offset..offset + name_len].copy_from_slice(self.data);
                offset += name_len;

                // attributes:  name="value"
                let mut attr_counter = 0;
                loop {
                    match self.attributes.get(attr_counter) {
                        Some((attr_name, attr_value)) => {
                            let name_len = attr_name.len();
                            let value_len = attr_value.len();

                            // space + name + = + " + value + "
                            let needed = 1 + name_len + 1 + 1 + value_len + 1;
                            if buffer_size < offset + needed {
                                return buf_too_small;
                            }

                            // space
                            out[offset] = b' ';
                            offset += 1;

                            // name
                            out[offset..offset + name_len].copy_from_slice(attr_name);
                            offset += name_len;

                            // =
                            out[offset] = b'=';
                            offset += 1;

                            // opening quote
                            out[offset] = b'"';
                            offset += 1;

                            // value
                            out[offset..offset + value_len].copy_from_slice(attr_value);
                            offset += value_len;

                            // closing quote
                            out[offset] = b'"';
                            offset += 1;

                            attr_counter += 1;
                        }
                        None => break,
                    }
                }

                // closing '>'
                if buffer_size < offset + 1 {
                    return buf_too_small;
                }
                out[offset] = b'>';
                offset += 1;

                Ok(offset)
            }
            _ => Err("Not a start element"),
        }
    }
}

fn write_event_to_buffer(event: &Event, buffer: &mut Vec<u8>, temp_buf: &mut [u8]) {
    match event.event_type {
        EventType::StartElement => {
            // Try using write_start_element with temp buffer
            match event.write_start_element(temp_buf) {
                Ok(len) => buffer.extend_from_slice(&temp_buf[0..len]),
                Err(_) => {
                    // Fallback: build manually for oversized elements
                    buffer.push(b'<');
                    buffer.extend_from_slice(event.data);
                    let mut i = 0;
                    while let Some((name, value)) = event.attributes.get(i) {
                        buffer.push(b' ');
                        buffer.extend_from_slice(name);
                        buffer.extend_from_slice(b"=\"");
                        buffer.extend_from_slice(value);
                        buffer.push(b'"');
                        i += 1;
                    }
                    buffer.push(b'>');
                }
            }
        }
        EventType::EndElement => {
            buffer.extend_from_slice(b"</");
            buffer.extend_from_slice(event.data);
            buffer.push(b'>');
        }
        EventType::Text => {
            buffer.extend_from_slice(event.data);
        }
        EventType::Comment => {
            buffer.extend_from_slice(b"<!--");
            buffer.extend_from_slice(event.data);
            buffer.extend_from_slice(b"-->");
        }
        EventType::CData => {
            buffer.extend_from_slice(b"<![CDATA[");
            buffer.extend_from_slice(event.data);
            buffer.extend_from_slice(b"]]>");
        }
        EventType::ProcessingInstruction => {
            buffer.extend_from_slice(b"<?");
            buffer.extend_from_slice(event.data);
            buffer.extend_from_slice(b"?>");
        }
        _ => {} // Fault, Eof
    }
}
