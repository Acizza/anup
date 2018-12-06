use crate::error::InputError;
use std::fmt::{Debug, Display};
use std::io;
use std::str::FromStr;

pub fn read_line() -> io::Result<String> {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;

    // Trim the right side of the buffer to remove the newline character
    Ok(buffer.trim_right().to_string())
}

pub fn read_range<T>(min: T, max: T) -> Result<T, InputError>
where
    T: PartialOrd + FromStr + Debug + Display + Copy,
    <T as FromStr>::Err: Debug,
{
    loop {
        let input = read_line()
            .map_err(InputError::ReadFailed)?
            .parse()
            .map_err(|e| InputError::ParseFailed(format!("{:?}", e)))?;

        if input >= min && input <= max {
            return Ok(input);
        } else {
            println!("input must be between {}-{}", min, max);
        }
    }
}

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

pub fn read_yn(default: Answer) -> io::Result<bool> {
    match read_line()?.as_str() {
        "y" | "Y" => Ok(true),
        "n" | "N" => Ok(false),
        _ => Ok(default.into()),
    }
}
