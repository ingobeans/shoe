use std::{
    error::Error,
    fs,
    io::{stdout, Read},
};

use crossterm::{
    cursor, execute, queue,
    style::{Color, SetForegroundColor},
    terminal,
};

use crate::colors;

fn ls(args: &[&String]) -> Result<CommandResult, Box<dyn Error>> {
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
    Ok(CommandResult::Lovely)
}

fn cd(args: &[&String]) -> Result<CommandResult, Box<dyn Error>> {
    let path = args.first();
    if let Some(path) = path {
        let path = shellexpand::tilde(path).to_string();
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() {
            Err(std::io::Error::other("Path is a file"))?
        }
        std::env::set_current_dir(path)?;
    }
    Ok(CommandResult::UpdateCwd)
}

fn pwd() -> Result<CommandResult, Box<dyn Error>> {
    queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
    println!(
        "{}",
        std::env::current_dir()?
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
    );
    Ok(CommandResult::Lovely)
}
fn echo(args: &Vec<&String>) -> Result<CommandResult, Box<dyn Error>> {
    queue!(stdout(), SetForegroundColor(Color::Reset))?;
    for line in args {
        println!("{}", line);
    }
    Ok(CommandResult::Lovely)
}
fn cls() -> Result<CommandResult, Box<dyn Error>> {
    execute!(
        stdout(),
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    Ok(CommandResult::Lovely)
}
fn cat(args: &[&String]) -> Result<CommandResult, Box<dyn Error>> {
    let path = args.first();
    match path {
        Some(path) => {
            let mut file = fs::File::open(path)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;

            queue!(stdout(), SetForegroundColor(Color::Reset))?;
            println!("{}", buf);
        }
        None => {
            println!("meow");
        }
    }

    Ok(CommandResult::Lovely)
}
fn help() -> Result<CommandResult, Box<dyn Error>> {
    println!("{}", include_str!("help.txt"));
    Ok(CommandResult::Lovely)
}

pub fn execute_command(keyword: &str, args: &Vec<&String>) -> CommandResult {
    match keyword {
        "ls" => handle_result(ls(args)),
        "cd" => handle_result(cd(args)),
        "pwd" => handle_result(pwd()),
        "echo" => handle_result(echo(args)),
        "cls" => handle_result(cls()),
        "cat" => handle_result(cat(args)),
        "help" => handle_result(help()),
        "exit" => CommandResult::Exit,
        _ => CommandResult::NotACommand,
    }
}

pub enum CommandResult {
    Lovely,
    Exit,
    UpdateCwd,
    Error,
    NotACommand,
}

pub fn handle_result(result: Result<CommandResult, Box<dyn Error>>) -> CommandResult {
    match result {
        Err(error) => {
            let _ = queue!(stdout(), SetForegroundColor(colors::ERR_COLOR));
            println!("{}", error);
            CommandResult::Error
        }
        Ok(result) => result,
    }
}
