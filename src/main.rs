use ripx::query::{PathQuery, PathSelector, run_query};
use ripx::reader::Reader;

fn main() -> std::io::Result<()> {
    let query_str = std::env::args().nth(1).expect("need query");
    let file_name = std::env::args().nth(2).unwrap();
    let file = std::fs::File::open(&file_name)?;
    let reader = std::io::BufReader::new(file);

    let mut xml_reader = Reader::from_reader(reader);
    let selector = PathSelector::parse(&query_str).unwrap();
    let mut query = PathQuery::new(selector, 10);

    run_query(&mut xml_reader, &mut query)?;
    Ok(())
}
