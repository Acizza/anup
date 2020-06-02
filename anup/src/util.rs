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

pub fn ms_from_mins<F>(mins: F) -> String
where
    F: Into<f32>,
{
    let mins = mins.into();
    let m = mins.floor() as u32;
    let s = (mins * 60.0 % 60.0).floor() as u32;

    format!("{:02}:{:02}", m, s)
}

pub fn hm_from_mins<F>(mins: F) -> String
where
    F: Into<f32>,
{
    let mins = mins.into();
    let h = (mins / 60.0).floor() as u32;
    let m = (mins % 60.0).floor() as u32;

    format!("{:02}:{:02}H", h, m)
}
