use std::{error::Error, fs, io::stdout, path::PathBuf};

use crossterm::{
    queue,
    style::{Color, SetForegroundColor},
};

use crate::colors;

fn ls(args: &Vec<&String>) -> Result<(), Box<dyn Error>> {
    let items = fs::read_dir(args.first().unwrap_or(&&".".to_string()))?;

    let mut dirs = vec![];
    let mut files = vec![];
    for item in items.flatten() {
        let name = item.file_name().into_string();
        match name {
            Ok(name) => {
                if item.file_type()?.is_file() {
                    files.push(name);
                } else {
                    dirs.push(name)
                }
            }
            Err(_) => Err(std::io::Error::other("Path name couldn't be read"))?,
        }
    }
    queue!(stdout(), SetForegroundColor(colors::PRIMARY_COLOR))?;
    for dir in dirs {
        println!("{}", dir)
    }
    queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
    for dir in files {
        println!("{}", dir)
    }
    Ok(())
}

fn cd(args: &Vec<&String>, cwd: &mut PathBuf) -> Result<(), Box<dyn Error>> {
    let path = args.first();
    if let Some(path) = path {
        let metadata = fs::metadata(path)?;
        if metadata.is_file() {
            Err(std::io::Error::other("Path is a file"))?
        }
        std::env::set_current_dir(path)?;
        *cwd = std::env::current_dir()?;
    }
    Ok(())
}

fn pwd(cwd: &mut PathBuf) -> Result<(), Box<dyn Error>> {
    queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
    println!(
        "{}",
        cwd.to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
    );
    Ok(())
}
fn echo(args: &Vec<&String>) -> Result<(), Box<dyn Error>> {
    queue!(stdout(), SetForegroundColor(Color::Reset))?;
    for line in args {
        println!("{}", line);
    }
    Ok(())
}

pub fn execute_command(keyword: &str, args: &Vec<&String>, cwd: &mut PathBuf) -> bool {
    match keyword {
        "ls" => handle_result(ls(args)),
        "cd" => handle_result(cd(args, cwd)),
        "pwd" => handle_result(pwd(cwd)),
        "echo" => handle_result(echo(args)),
        _ => false,
    }
}

pub fn handle_result(result: Result<(), Box<dyn Error>>) -> bool {
    if let Err(error) = result {
        let _ = queue!(stdout(), SetForegroundColor(colors::ERR_COLOR));
        println!("{}", error);
    }
    true
}
