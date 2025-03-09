use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crossterm::{
    cursor, execute, queue,
    style::{Color, SetForegroundColor},
    terminal,
};

use crate::utils::{Theme, THEMES};

fn ls(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
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
    queue!(
        context.stdout,
        SetForegroundColor(context.theme.primary_color)
    )?;
    for dir in dirs {
        writeln!(context.stdout, "{}", dir)?;
    }
    queue!(context.stdout, SetForegroundColor(Color::Reset))?;
    for dir in files {
        writeln!(context.stdout, "{}", dir)?;
    }
    Ok(CommandResult::Lovely)
}

fn cd(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    let path = context.args.front();
    if let Some(path) = path {
        let path = shellexpand::tilde(path).to_string();
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() {
            Err(std::io::Error::other("Path is a file"))?
        }
        std::env::set_current_dir(path)?;
    }
    Ok(CommandResult::Lovely)
}

fn pwd(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    writeln!(
        context.stdout,
        "{}",
        std::env::current_dir()?
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
    )?;
    Ok(CommandResult::Lovely)
}
fn echo(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    queue!(context.stdout, SetForegroundColor(Color::Reset))?;
    for line in context.args {
        writeln!(context.stdout, "{}", line)?;
    }
    Ok(CommandResult::Lovely)
}
fn cls(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    execute!(
        context.stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    Ok(CommandResult::Lovely)
}
fn cat(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
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
fn help(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    writeln!(context.stdout, "{}", include_str!("help.txt"))?;
    Ok(CommandResult::Lovely)
}
fn cp(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    if context.args.len() != 2 {
        Err(std::io::Error::other("Usage: 'cp <source> <dest>'"))?;
    }
    let source = context.args[0];
    let mut dest_pathbuf: PathBuf = context.args[1].into();

    let source_is_file = fs::metadata(source)?.is_file();

    // is destination the path to an existing directory?
    let dest_is_existing_directory = if let Ok(metadata) = fs::metadata(&dest_pathbuf) {
        metadata.is_dir()
    } else {
        false
    };

    // if source is a file, and destination is a directory (without filename), append the source filename to the destination path
    if source_is_file && dest_is_existing_directory {
        let source_pathbuf: PathBuf = source.into();
        dest_pathbuf.push(source_pathbuf.file_name().unwrap());
    }

    if source_is_file {
        std::fs::copy(source, dest_pathbuf)?;
    } else {
        copy_dir(source, dest_pathbuf)?;
    }
    Ok(CommandResult::Lovely)
}
fn mv(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
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
fn rm(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
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
fn mkdir(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    if context.args.len() != 1 {
        Err(std::io::Error::other("Usage: 'mkdir <path>'"))?;
    }
    let path = context.args[0];
    fs::create_dir_all(path)?;
    Ok(CommandResult::Lovely)
}
fn theme(context: &mut CommandContext) -> Result<CommandResult, std::io::Error> {
    if context.args.len() != 1 {
        writeln!(context.stdout, "Usage: 'theme <theme name>'")?;
        writeln!(context.stdout, "Available themes: ")?;
        for theme in THEMES {
            // if theme is active theme, print with color
            if std::ptr::eq(theme, context.theme) {
                queue!(
                    context.stdout,
                    SetForegroundColor(context.theme.primary_color)
                )?;
                writeln!(context.stdout, "\t* {}", theme.name)?;
                queue!(context.stdout, SetForegroundColor(Color::Reset))?;
            } else {
                writeln!(context.stdout, "\t* {}", theme.name)?;
            }
        }
        Ok(CommandResult::Lovely)
    } else {
        let theme_name = context.args[0];

        for (index, theme) in THEMES.iter().enumerate() {
            if theme.name == theme_name {
                return Ok(CommandResult::UpdateTheme(index));
            }
        }
        let message = format!("no theme by name '{}'", theme_name);
        Err(std::io::Error::other(message))
    }
}

pub fn execute_command(
    keyword: &str,
    context: &mut CommandContext,
) -> Result<CommandResult, std::io::Error> {
    match keyword {
        "ls" => ls(context),
        "cd" => cd(context),
        "pwd" => pwd(context),
        "echo" => echo(context),
        "cls" => cls(context),
        "cat" => cat(context),
        "cp" => cp(context),
        "mv" => mv(context),
        "rm" => rm(context),
        "help" => help(context),
        "mkdir" => mkdir(context),
        "theme" => theme(context),
        "exit" => Ok(CommandResult::Exit),
        _ => Ok(CommandResult::NotACommand),
    }
}

pub struct CommandContext<'a> {
    pub args: &'a VecDeque<&'a str>,
    pub theme: &'a Theme<'a>,
    pub stdout: &'a mut Vec<u8>,
}

pub enum CommandResult {
    Lovely,
    Exit,
    UpdateTheme(usize),
    NotACommand,
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
