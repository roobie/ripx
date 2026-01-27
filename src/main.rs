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

    // accumulator for "current element of interest"
    let mut acc: Vec<u8> = Vec::with_capacity(1024 * 64);
    let mut collect_data = false;
    loop {
        let ev = pp.next_event()?;
        // print!("{:?}|", ev.event_type);
        // println!("{}", String::from_utf8_lossy(ev.data));
        match ev.event_type {
            EventType::StartElement => {
                if ev.data != element_name_bytes {
                    continue;
                }
                collect_data = true;
                // we are in an element of interest
                match_counter += 1;
                // Caller can put out on the stack (e.g.
                let mut buf = [0u8; 1024];
                let len = ev.write_start_element(&mut buf).unwrap();
                println!("{}", String::from_utf8_lossy(&buf[0..len]));
            }
            EventType::Text => {
                if !collect_data {
                    continue;
                }
                let trimmed = trim_ascii_whitespace(ev.data);
                if trimmed.len() > 0 {
                    println!("[{}]", String::from_utf8_lossy(ev.data));
                }
            }
            EventType::EndElement => {
                if !collect_data {
                    continue;
                }
                println!("</{}>", String::from_utf8_lossy(ev.data));
            }
            EventType::Fault => {
                println!("FAULT: {:?} {}", ev.error, String::from_utf8_lossy(ev.data));
            }
            _ => {
                break;
            }
        };
    }

    Ok(())
}

fn trim_ascii_whitespace(data: &[u8]) -> &[u8] {
    let start = data
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(data.len()); // all whitespace
    let end = data
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|i| i + 1)
        .unwrap_or(start);
    &data[start..end]
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
