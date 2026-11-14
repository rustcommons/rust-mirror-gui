use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Line {
    word: String,
    count: u64,
}

fn main() -> anyhow::Result<()> {
    let text = "mirror crates mirror offline";
    let re = regex::Regex::new(r"\bmirror\b")?;
    let count = re.find_iter(text).count() as u64;
    let line = Line { word: "mirror".into(), count };
    let json = serde_json::to_string(&line)?;
    let back: Line = serde_json::from_str(&json)?;
    let mut buf = itoa::Buffer::new();
    println!(
        "{} {} {}",
        json,
        buf.format(back.count),
        memchr::memchr(b'o', text.as_bytes()).unwrap_or(0)
    );
    Ok(())
}
