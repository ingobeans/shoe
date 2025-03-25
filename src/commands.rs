use std::{
    collections::VecDeque,
    fs,
    io::{Read, Result, Write},
    path::{Path, PathBuf},
};

use crossterm::{
    cursor, execute, queue,
    style::{Color, SetForegroundColor},
    terminal,
};

use crate::utils::{Theme, THEMES};

fn match_pattern(entries: &[String], pattern: &str) -> Vec<String> {
    if let Some(split) = pattern.split_once("*") {
        let (startswith, endswith) = split;
        let g = entries
            .iter()
            .filter(|f| f.starts_with(startswith) && f.ends_with(endswith))
            .map(|f| f.to_string())
            .collect();
        g
    } else {
        for item in entries {
            if *item == pattern {
                return vec![item.to_string()];
            }
        }
        Vec::new()
    }
}

fn match_file_pattern(pattern: &str) -> Result<(Vec<String>, PathBuf)> {
    let source = pattern;
    let source_pathbuf: PathBuf = source.into();

    let source_filename = match source_pathbuf.file_name() {
        Some(name) => name.to_string_lossy().to_string(),
        None => "".to_string(),
    };

    let source_parent = match source_pathbuf.parent() {
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(""),
    };

    let mut source_parent_string = source_parent.to_string_lossy().to_string();
    // if parent is "", replace with ".", so std::fs::read_dir works right
    if source_parent_string.is_empty() {
        source_parent_string = ".".to_string();
    }

    // get contents of source dir (for pattern matching)
    let source_dir_contents: Vec<String> = std::fs::read_dir(source_parent_string)?
        .flatten()
        .map(|f| f.file_name().to_string_lossy().to_string())
        .collect();

    let matches = match_pattern(&source_dir_contents, &source_filename);
    Ok((matches, source_parent))
}

fn ls(context: &mut CommandContext) -> Result<CommandResult> {
    let path = context.args.front().unwrap_or(&".");
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.is_file() {
            Err(std::io::Error::other("Path is a file"))?
        }
    }
    let items = fs::read_dir(path)?;

    let mut dirs = vec![];
    let mut files = vec![];
    for item in items.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if item.file_type()?.is_file() {
            files.push(name);
        } else {
            dirs.push(name)
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

fn cd(context: &mut CommandContext) -> Result<CommandResult> {
    let path = context.args.front();
    if let Some(path) = path {
        let metadata = fs::metadata(path)?;
        if metadata.is_file() {
            Err(std::io::Error::other("Path is a file"))?
        }
        std::env::set_current_dir(path)?;
    }
    Ok(CommandResult::Lovely)
}

fn pwd(context: &mut CommandContext) -> Result<CommandResult> {
    writeln!(
        context.stdout,
        "{}",
        std::env::current_dir()?
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
    )?;
    Ok(CommandResult::Lovely)
}
fn echo(context: &mut CommandContext) -> Result<CommandResult> {
    queue!(context.stdout, SetForegroundColor(Color::Reset))?;
    for line in context.args {
        writeln!(context.stdout, "{}", line)?;
    }
    Ok(CommandResult::Lovely)
}
fn cls(context: &mut CommandContext) -> Result<CommandResult> {
    execute!(
        context.stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    Ok(CommandResult::Lovely)
}
fn cat(context: &mut CommandContext) -> Result<CommandResult> {
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
fn help(context: &mut CommandContext) -> Result<CommandResult> {
    writeln!(context.stdout, "{}", include_str!("help.txt"))?;
    Ok(CommandResult::Lovely)
}
fn cp(context: &mut CommandContext) -> Result<CommandResult> {
    if context.args.len() != 2 {
        Err(std::io::Error::other("Usage: 'cp <source> <dest>'"))?;
    }
    let dest_pathbuf = PathBuf::from(context.args[1]);
    let (matches, source_parent) = match_file_pattern(context.args[0])?;
    let more_than_1_match = matches.len() > 1;

    // if more than 1 match, validate that the output dest is a directory, and not direct file path
    if more_than_1_match && !dest_pathbuf.is_dir() {
        Err(std::io::Error::other(
            "Can't copy the files. Either destination directory doesn't exist, or source pattern matches multiple items, but destination is a single file.",
        ))?;
    }
    // if no matches, raise error
    if matches.is_empty() {
        Err(std::io::Error::other("Source item(s) not found."))?;
    }

    for name in matches {
        let mut dest_pathbuf = dest_pathbuf.clone();
        let source = source_parent.join(&name);

        let source_is_file = source.is_file();

        // if source is a file, and destination is a directory (without filename), append the source filename to the destination path
        if more_than_1_match || (source_is_file && dest_pathbuf.is_dir()) {
            dest_pathbuf.push(&name);
        }

        println!("{:?} -> {:?}", source, dest_pathbuf);

        if source_is_file {
            std::fs::copy(&source, dest_pathbuf)?;
        } else {
            copy_dir(&source, dest_pathbuf)?;
        }
    }

    Ok(CommandResult::Lovely)
}
fn mv(context: &mut CommandContext) -> Result<CommandResult> {
    if context.args.len() != 2 {
        Err(std::io::Error::other("Usage: 'mv <source> <dest>'"))?;
    }
    cp(context)?;
    let (matches, source_parent) = match_file_pattern(context.args[0])?;
    for name in matches {
        let pathbuf = source_parent.join(name);
        if pathbuf.is_file() {
            std::fs::remove_file(pathbuf)?;
        } else {
            std::fs::remove_dir_all(pathbuf)?;
        }
    }
    Ok(CommandResult::Lovely)
}
fn rm(context: &mut CommandContext) -> Result<CommandResult> {
    if context.args.len() != 1 {
        Err(std::io::Error::other("Usage: 'rm <target>'"))?;
    }
    let (matches, source_parent) = match_file_pattern(context.args[0])?;
    for name in matches {
        let pathbuf = source_parent.join(name);
        let target_is_file = pathbuf.is_file();
        if target_is_file {
            std::fs::remove_file(pathbuf)?;
        } else {
            std::fs::remove_dir_all(pathbuf)?;
        }
    }
    Ok(CommandResult::Lovely)
}
fn mkdir(context: &mut CommandContext) -> Result<CommandResult> {
    if context.args.len() != 1 {
        Err(std::io::Error::other("Usage: 'mkdir <path>'"))?;
    }
    let path = context.args[0];
    fs::create_dir_all(path)?;
    Ok(CommandResult::Lovely)
}
fn theme(context: &mut CommandContext) -> Result<CommandResult> {
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

pub fn execute_command(keyword: &str, context: &mut CommandContext) -> Result<CommandResult> {
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
