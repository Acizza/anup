use failure::Error;
use std::io;

pub fn read_line() -> Result<String, Error> {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;

    Ok(buffer[..buffer.len() - 1].to_string())
}

pub fn read_usize_range(min: usize, max: usize) -> Result<usize, Error> {
    loop {
        let input = read_line()?.parse()?;

        if input >= min && input <= max {
            return Ok(input);
        } else {
            println!("input must be between {}-{}", min, max);
        }
    }
}

#[derive(Debug)]
pub enum Answer {
    Yes,
    No,
}

impl Into<bool> for Answer {
    fn into(self) -> bool {
        match self {
            Answer::Yes => true,
            Answer::No => false,
        }
    }
}

pub fn read_yn(default: Answer) -> Result<bool, Error> {
    let line = read_line()?;

    let answer = match line.as_str() {
        "Y" | "y" => true,
        "N" | "n" => false,
        _ => default.into(),
    };

    Ok(answer)
}