use std::{
    collections::VecDeque,
    error::Error,
    fs,
    io::{stdout, Read, Write},
    path::Path,
};

use crossterm::{
    cursor, execute, queue,
    style::{Color, SetForegroundColor},
    terminal,
};

use crate::colors;

fn ls(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    let items = fs::read_dir(context.args.front().unwrap_or(&"."))?;

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
    queue!(context.stdout, SetForegroundColor(colors::PRIMARY_COLOR))?;
    for dir in dirs {
        writeln!(context.stdout, "{}", dir)?;
    }
    queue!(context.stdout, SetForegroundColor(colors::SECONDARY_COLOR))?;
    for dir in files {
        writeln!(context.stdout, "{}", dir)?;
    }
    Ok(CommandResult::Lovely)
}

fn cd(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    let path = context.args.front();
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

fn pwd(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    writeln!(
        context.stdout,
        "{}",
        std::env::current_dir()?
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
    )?;
    Ok(CommandResult::Lovely)
}
fn echo(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    queue!(context.stdout, SetForegroundColor(Color::Reset))?;
    for line in context.args {
        writeln!(context.stdout, "{}", line)?;
    }
    Ok(CommandResult::Lovely)
}
fn cls(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    execute!(
        context.stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    Ok(CommandResult::Lovely)
}
fn cat(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    let path = context.args.front();
    match path {
        Some(path) => {
            let mut file = fs::File::open(path)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;

            queue!(context.stdout, SetForegroundColor(Color::Reset))?;
            writeln!(context.stdout, "{}", buf)?;
        }
        None => {
            writeln!(context.stdout, "meow")?;
        }
    }

    Ok(CommandResult::Lovely)
}
fn help(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    writeln!(context.stdout, "{}", include_str!("help.txt"))?;
    Ok(CommandResult::Lovely)
}
fn cp(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    if context.args.len() != 2 {
        Err(std::io::Error::other("Usage: 'cp <source> <dest>'"))?;
    }
    let source = context.args[0];
    let dest = context.args[1];

    let source_is_file = fs::metadata(source)?.is_file();
    if source_is_file {
        std::fs::copy(source, dest)?;
    } else {
        copy_dir(source, dest)?;
    }
    Ok(CommandResult::Lovely)
}
fn mv(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    if context.args.len() != 2 {
        Err(std::io::Error::other("Usage: 'mv <source> <dest>'"))?;
    }
    cp(context)?;
    let source = context.args[0];
    let source_is_file = fs::metadata(source)?.is_file();
    if source_is_file {
        std::fs::remove_file(source)?;
    } else {
        std::fs::remove_dir_all(source)?;
    }
    Ok(CommandResult::Lovely)
}
fn rm(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    if context.args.len() != 1 {
        Err(std::io::Error::other("Usage: 'rm <target>'"))?;
    }
    let target = context.args[0];
    let target_is_file = fs::metadata(target)?.is_file();
    if target_is_file {
        std::fs::remove_file(target)?;
    } else {
        std::fs::remove_dir_all(target)?;
    }
    Ok(CommandResult::Lovely)
}
fn mkdir(context: &mut CommandContext) -> Result<CommandResult, Box<dyn Error>> {
    if context.args.len() != 1 {
        Err(std::io::Error::other("Usage: 'mkdir <path>'"))?;
    }
    let path = context.args[0];
    fs::create_dir_all(path)?;
    Ok(CommandResult::Lovely)
}

pub fn execute_command(keyword: &str, context: &mut CommandContext) -> CommandResult {
    match keyword {
        "ls" => handle_result(ls(context)),
        "cd" => handle_result(cd(context)),
        "pwd" => handle_result(pwd(context)),
        "echo" => handle_result(echo(context)),
        "cls" => handle_result(cls(context)),
        "cat" => handle_result(cat(context)),
        "cp" => handle_result(cp(context)),
        "mv" => handle_result(mv(context)),
        "rm" => handle_result(rm(context)),
        "help" => handle_result(help(context)),
        "mkdir" => handle_result(mkdir(context)),
        "exit" => CommandResult::Exit,
        _ => CommandResult::NotACommand,
    }
}

pub struct CommandContext<'a> {
    pub args: &'a VecDeque<&'a str>,
    pub stdout: &'a mut Vec<u8>,
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

fn copy_dir(source: impl AsRef<Path>, dest: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dest)?;
    for item in fs::read_dir(source)?.flatten() {
        let is_file = item.file_type()?.is_file();
        if is_file {
            fs::copy(item.path(), dest.as_ref().join(item.file_name()))?;
        } else {
            copy_dir(item.path(), dest.as_ref().join(item.file_name()))?;
        }
    }
    Ok(())
}
