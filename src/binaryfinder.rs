use std::{collections::HashMap, env, io::Result, path::PathBuf};

use relative_path::RelativePathBuf;

pub fn get_script_runtime(script_extension: &str) -> Option<&str> {
    Some(match script_extension {
        "py" => "python3",
        "js" => "node",
        "vbs" => "wscript",
        _ => return None,
    })
}

pub fn get_path_extensions() -> Vec<String> {
    if env::consts::OS != "windows" {
        Vec::new()
    } else {
        vec![
            String::from(".exe"),
            String::from(".bat"),
            String::from(".cmd"),
        ]
    }
}

pub fn get_path_variants(path: &str, path_extensions: &Vec<String>) -> Vec<String> {
    if env::consts::OS != "windows" {
        vec![path.to_owned()]
    } else {
        let mut variants = Vec::new();

        variants.push(path.to_string());
        for extension in path_extensions {
            variants.push(path.to_string() + &extension);
        }
        variants
    }
}

pub fn get_items_in_path() -> HashMap<String, PathBuf> {
    let path = env::var_os("PATH").unwrap_or_default();
    let paths = env::split_paths(&path).collect::<Vec<_>>();
    let mut items: HashMap<String, PathBuf> = HashMap::new();
    for item in paths {
        if !item.is_dir() {
            continue;
        }
        for subitem in item.read_dir().unwrap().flatten() {
            if subitem.file_type().unwrap().is_file() {
                let name = subitem
                    .file_name()
                    .to_string_lossy()
                    .to_string()
                    .to_lowercase();

                let full_path: PathBuf = item.join(&name);
                if !items.contains_key(&name) {
                    items.insert(name, full_path);
                }
            }
        }
    }
    items
}

pub fn find_binary(
    input: &str,
    path_items: &HashMap<String, PathBuf>,
    path_extensions: &Vec<String>,
) -> Result<PathBuf> {
    let cwd = env::current_dir()?;

    // check all variations if they exist (relative to cwd)
    let variations = get_path_variants(input, path_extensions);
    for variation in &variations {
        let pathbuf = {
            let pathbuf = PathBuf::from(variation);
            if pathbuf.is_absolute() {
                pathbuf
            } else {
                RelativePathBuf::from(variation).to_logical_path(&cwd)
            }
        };
        if pathbuf.is_file() {
            return Ok(pathbuf);
        }
    }
    // if none exists, check PATH
    for variation in &variations {
        if let Some(pathbuf) = path_items.get(variation) {
            return Ok(pathbuf.clone());
        }
    }
    // if all else fails, return original input
    Ok(input.into())
}
