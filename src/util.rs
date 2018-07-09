use directories::ProjectDirs;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

lazy_static! {
    static ref PROJECT_DIRS: ProjectDirs = {
        let dirs = ProjectDirs::from("", "", env!("CARGO_PKG_NAME"));

        match dirs {
            Some(dirs) => dirs,
            None => panic!("failed to get user directories"),
        }
    };
}

fn get_valid_dir_path(dir: &Path, file: &str) -> io::Result<PathBuf> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }

    let mut path = PathBuf::from(dir);
    path.push(file);

    Ok(path)
}

pub fn get_valid_config_path(file: &str) -> io::Result<PathBuf> {
    get_valid_dir_path(PROJECT_DIRS.config_dir(), file)
}

pub fn concat_sequential_values(
    list: &[u32],
    group_delim: &str,
    space_delim: &str,
) -> Option<String> {
    match list.len() {
        0 => return None,
        1 => return Some(list[0].to_string()),
        _ => (),
    }

    let mut concat_str = list[0].to_string();
    let mut group_start_val = list[0];
    let mut prev_value = list[0];

    for &value in list {
        // Check for a nonsequential jump
        if (value as i32 - prev_value as i32).abs() > 1 {
            // Extend the current group with a range if there's a big enough gap between the current value
            // and start value of the group
            if (value as i32 - group_start_val as i32).abs() > 2 {
                concat_str.push_str(group_delim);
                concat_str.push_str(&prev_value.to_string());
            }

            // Form a new group
            concat_str.push_str(space_delim);
            concat_str.push_str(&value.to_string());

            group_start_val = value;
        }

        prev_value = value;
    }

    let last_item = list[list.len() - 1];

    // Finish off the last list item with a range if it extends beyond the start value of the current group
    if group_start_val != last_item {
        concat_str.push_str(group_delim);
        concat_str.push_str(&last_item.to_string());
    }

    Some(concat_str)
}
