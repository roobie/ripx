pub mod query;
pub mod reader;
pub mod tokenizer;

pub enum Event {
    StartElement {
        name: String,
        attributes: Vec<(String, String)>,
    },
    EndElement {
        name: String,
    },
    Text(String),
    Comment(String), // minimal
    CData(String),   // minimal
    Eof,
}
