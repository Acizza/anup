use std::io;

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        ParseInt(::std::num::ParseIntError);
    }
}

pub fn read_line() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;

    Ok(buffer[..buffer.len() - 1].to_string())
}

pub fn read_usize_range(min: usize, max: usize) -> Result<usize> {
    loop {
        let input = read_line()?.parse()?;

        if input >= min && input <= max {
            return Ok(input)
        } else {
            println!("input must be between {}-{}", min, max);
        }
    }
}