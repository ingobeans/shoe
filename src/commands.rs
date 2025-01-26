use std::{
    error::Error,
    fs,
    io::{stdin, stdout, Read, Stdin, Stdout, Write},
};

use crossterm::{
    cursor, execute, queue,
    style::{Color, SetForegroundColor},
    terminal,
};

use crate::colors;

fn ls(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    let items = fs::read_dir(context.args.first().unwrap_or(&&".".to_string()))?;

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
        writeln!(context.stdout.lock(), "{}", dir)?;
    }
    queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
    for dir in files {
        writeln!(context.stdout.lock(), "{}", dir)?;
    }
    Ok(CommandResult::Lovely)
}

fn cd(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    let path = context.args.first();
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

fn pwd(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
    writeln!(
        context.stdout.lock(),
        "{}",
        std::env::current_dir()?
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
    )?;
    Ok(CommandResult::Lovely)
}
fn echo(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    queue!(stdout(), SetForegroundColor(Color::Reset))?;
    for line in context.args {
        writeln!(context.stdout.lock(), "{}", line)?;
    }
    Ok(CommandResult::Lovely)
}
fn cls(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    execute!(
        context.stdout.lock(),
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    Ok(CommandResult::Lovely)
}
fn cat(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    let path = context.args.first();
    match path {
        Some(path) => {
            let mut file = fs::File::open(path)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;

            queue!(stdout(), SetForegroundColor(Color::Reset))?;
            writeln!(context.stdout.lock(), "{}", buf)?;
        }
        None => {
            writeln!(context.stdout.lock(), "meow")?;
        }
    }

    Ok(CommandResult::Lovely)
}
fn help(context: CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    writeln!(context.stdout.lock(), "{}", include_str!("help.txt"))?;
    Ok(CommandResult::Lovely)
}

pub fn execute_command(keyword: &str, args: &Vec<&String>) -> CommandResult {
    let context = CommandContext {
        args: args,
        _stdin: stdin(),
        stdout: stdout(),
    };
    match keyword {
        "ls" => handle_result(ls(context)),
        "cd" => handle_result(cd(context)),
        "pwd" => handle_result(pwd(context)),
        "echo" => handle_result(echo(context)),
        "cls" => handle_result(cls(context)),
        "cat" => handle_result(cat(context)),
        "help" => handle_result(help(context)),
        "exit" => CommandResult::Exit,
        _ => CommandResult::NotACommand,
    }
}

pub struct CommandContext<'a> {
    args: &'a Vec<&'a String>,
    stdout: Stdout,
    _stdin: Stdin,
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
