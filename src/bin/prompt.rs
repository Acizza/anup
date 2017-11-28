use failure::{Error, ResultExt};
use mal::{SeriesInfo, MAL};

mod input {
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
}

pub fn find_and_select_series(mal: &MAL, name: &str) -> Result<SeriesInfo, Error> {
    let mut series = mal.search(name).context("MAL search failed")?;

    if series.len() == 0 {
        return Err(format_err!("no anime named [{}] found", name));
    } else if series.len() > 1 {
        println!("found multiple anime named [{}] on MAL", name);
        println!("input the number corrosponding with the intended anime:\n");

        for (i, s) in series.iter().enumerate() {
            println!("{} [{}]", 1 + i, s.title);
        }

        let idx = input::read_usize_range(1, series.len())? - 1;
        Ok(series.swap_remove(idx))
    } else {
        Ok(series.swap_remove(0))
    }
}
