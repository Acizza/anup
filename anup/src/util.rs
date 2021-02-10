#[macro_export]
macro_rules! try_opt_r {
    ($x:expr) => {
        match $x {
            Some(value) => value,
            None => return Ok(()),
        }
    };
}

#[macro_export]
macro_rules! try_opt_ret {
    ($x:expr) => {
        match $x {
            Some(value) => value,
            None => return,
        }
    };
}

pub fn hm_from_mins<F>(total_mins: F) -> String
where
    F: Into<f32>,
{
    let total_mins = total_mins.into();

    let hours = (total_mins / 60.0).floor() as u8;
    let minutes = (total_mins % 60.0).floor() as u8;

    format!("{:02}:{:02}H", hours, minutes)
}
