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

pub fn read_int(min: i32, max: i32) -> Result<i32> {
    let mut input = read_line()?.parse()?;

    while input < min || input > max {
        println!("input must be between {}-{}", min, max);
        input = read_line()?.parse()?;
    }

    Ok(input)
}

#[derive(Debug)]
pub enum Answer {
    Yes,
    No
}

pub fn read_yn(def: Answer) -> Result<bool> {
    let input = read_line()?.to_lowercase();

    let non_default = match def {
        Answer::Yes => "n",
        Answer::No  => "y",
    };

    Ok(input != non_default)
}