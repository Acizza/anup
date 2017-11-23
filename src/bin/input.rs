use std::io;
use failure::Error;

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
