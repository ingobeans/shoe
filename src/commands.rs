//! Handles builtin commands

use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Read, Result, Write},
    path::{Path, PathBuf},
};

use copypasta::ClipboardProvider;
use crossterm::{
    cursor, execute, queue,
    style::{Color, SetForegroundColor},
    terminal,
};

use crate::{
    absolute_pathbuf_to_string, binaryfinder,
    utils::{Theme, THEMES},
};

/// Matches a string pattern with wildcards against a set of entries.
///
/// I.e. with the entries `["hello world", "cool world", "wahoo"]`, and the pattern `* world`, would yield `["hello world", "cool world"]`
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

/// Matches a string pattern with wildcards against items in the current working directory
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

/// Get "actual" width of a string. Counts the amount of characters, discarding invisible ansi codes.
fn width_of_string(input: &str) -> usize {
    let stripped = strip_ansi_escapes::strip_str(input);
    stripped.chars().count()
}

/// Breaks a string containing a list of items seperated by \n into columns accounting for the terminal width
fn into_columns(input: String) -> Result<String> {
    let (width, _) = crossterm::terminal::size()?;
    let width = width as usize;

    if !input.contains('\n') {
        return Ok(input);
    }

    let mut lines = input.split('\n');

    let mut longest_line = lines.next().unwrap();
    let mut longest_line_length = width_of_string(longest_line);
    for line in lines {
        let length = width_of_string(line);
        if length > width_of_string(longest_line) {
            longest_line = line;
            longest_line_length = length;
        }
    }
    longest_line_length += 1;

    let amount_of_items = input.split('\n').count();

    let columns = (width / longest_line_length).max(1);
    let rows = (amount_of_items / columns).max(1);

    let mut new = vec![String::new(); rows];
    for (index, line) in input.split('\n').enumerate() {
        let column_index = index / rows;
        let row_index = index % rows;

        let space_to_add = column_index * longest_line_length - width_of_string(&new[row_index]);

        new[row_index] += &(" ".repeat(space_to_add) + line);
    }

    Ok(new.join("\n"))
}

fn column(context: &mut CommandContext) -> Result<CommandResult> {
    let data = String::from_utf8_lossy(&context.stdin).to_string();
    let columns = into_columns(data)?;
    writeln!(context.stdout, "{}", columns)?;
    Ok(CommandResult::Lovely)
}

fn ls(context: &mut CommandContext) -> Result<CommandResult> {
    let path: PathBuf = context.args.front().unwrap_or(&".").into();

    if !path.exists() {
        Err(std::io::Error::other("Directory doesn't exist"))?
    }
    if path.is_file() {
        Err(std::io::Error::other("Path is a file"))?
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
    for dir in dirs {
        queue!(
            context.stdout,
            SetForegroundColor(context.theme.primary_color)
        )?;
        writeln!(context.stdout, "{}", dir)?;
    }
    for dir in files {
        queue!(context.stdout, SetForegroundColor(Color::Reset))?;
        writeln!(context.stdout, "{}", dir)?;
    }
    Ok(CommandResult::Lovely)
}
fn cd(context: &mut CommandContext) -> Result<CommandResult> {
    let path = context.args.front();
    if let Some(path) = path {
        let path = PathBuf::from(path);

        if !path.exists() {
            Err(std::io::Error::other("Directory doesn't exist"))?
        }
        if path.is_file() {
            Err(std::io::Error::other("Path is a file"))?
        }
        std::env::set_current_dir(path)?;
    }
    Ok(CommandResult::Lovely)
}
fn pwd(context: &mut CommandContext) -> Result<CommandResult> {
    let path = std::env::current_dir()?;

    let text = absolute_pathbuf_to_string(&path);
    writeln!(context.stdout, "{}", text)?;

    Ok(CommandResult::Lovely)
}
fn copy(context: &mut CommandContext) -> Result<CommandResult> {
    // to avoid panic
    if context.stdin.is_empty() {
        return Ok(CommandResult::Lovely);
    }

    // try decoding stdin as utf_8
    match std::str::from_utf8(&context.stdin) {
        Ok(text) => {
            // strip ansi codes and non text chars
            let text = strip_ansi_escapes::strip_str(text);
            // create a clipboard context
            let Ok(mut ctx) = copypasta::ClipboardContext::new() else {
                return Err(std::io::Error::other("Couldn't access clipboard"));
            };
            // write to clipboard context and handle error if it arises
            if ctx.set_contents(text.to_string()).is_err() {
                return Err(std::io::Error::other("Couldn't write to clipboard"));
            }
            Ok(CommandResult::Lovely)
        }
        Err(_) => Err(std::io::Error::other("Stdin was not UTF-8 text")),
    }
}
fn echo(context: &mut CommandContext) -> Result<CommandResult> {
    // dont add newline if -n or --no-newline
    let newline = !(context.args.contains(&"-n") || context.args.contains(&"--no-newline"));

    if !context.stdin.is_empty() {
        context.stdout.append(&mut context.stdin);
        return Ok(CommandResult::Lovely);
    }
    for line in context.args {
        if *line == "-n" || *line == "--no-newline" {
            continue;
        }

        let mut output: Vec<u8> = Vec::new();
        let mut last_was_backslash = false;
        let mut current_hex = None;

        // parse text for hex, like \x00
        for c in line.as_bytes() {
            if *c == b'\\' {
                last_was_backslash = true;
                output.push(*c);
            } else if *c == b'x' && last_was_backslash {
                current_hex = Some(Vec::new());
                output.push(*c);
            } else if let Some(value) = &mut current_hex {
                value.push(*c);
                let as_string = std::str::from_utf8(value);
                if as_string.is_err() {
                    current_hex = None;
                } else if as_string.unwrap().len() >= 2 {
                    let parsed = u8::from_str_radix(as_string.unwrap(), 16);
                    if let Ok(parsed) = parsed {
                        // if it is valid hex, write it, and remove last 2 chars of output to get rid of the "\x"
                        output.pop();
                        output.pop();
                        output.push(parsed);
                        current_hex = None;
                    } else {
                        // if the submitted hex isnt valid hex, write it as text instead
                        output.append(value);
                        current_hex = None;
                    }
                }
            } else {
                last_was_backslash = false;
                output.push(*c);
            }
        }

        context.stdout.append(&mut output);
        if newline {
            writeln!(context.stdout)?;
        }
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
    if matches.len() == 0 {
        Err(std::io::Error::other("No item by name/pattern found"))?;
    }
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
fn which(context: &mut CommandContext) -> Result<CommandResult> {
    if context.args.len() != 1 {
        Err(std::io::Error::other("Usage: 'which <binary>'"))?;
    }
    let name = context.args[0];
    let binary = binaryfinder::find_binary(name, context.path_items, context.path_extensions)?;
    writeln!(context.stdout, "{}", binary.to_string_lossy())?;
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
        let message = format!("No theme by name '{}'", theme_name);
        Err(std::io::Error::other(message))
    }
}

type CommandFunction = &'static dyn Fn(&mut CommandContext) -> Result<CommandResult>;

/// Const array of all builtin functions as key value pairs of their name and function reference
///
/// This is so builtin functions can be searched by name
pub const COMMANDS: &[(&str, CommandFunction)] = &[
    ("ls", &ls),
    ("cd", &cd),
    ("pwd", &pwd),
    ("echo", &echo),
    ("column", &column),
    ("cls", &cls),
    ("cat", &cat),
    ("cp", &cp),
    ("mv", &mv),
    ("rm", &rm),
    ("help", &help),
    ("mkdir", &mkdir),
    ("theme", &theme),
    ("which", &which),
    ("copy", &copy),
    ("exit", &|_| Ok(CommandResult::Exit)),
];

/// Try to execute a builtin command
///
/// Returns a [Result] holding a [CommandResult].
///
/// If the command doesn't exist, the function will return `Ok(CommandResult::NotACommand)`
pub fn execute_command(keyword: &str, context: &mut CommandContext) -> Result<CommandResult> {
    // Try to find builtin command by name of keyword
    for (name, command) in COMMANDS {
        if *name == keyword {
            return command(context);
        }
    }
    // None was found
    Ok(CommandResult::NotACommand)
}

/// Context passed to builtin commands
pub struct CommandContext<'a> {
    pub args: &'a VecDeque<&'a str>,
    pub theme: &'static Theme,
    pub stdout: &'a mut Vec<u8>,
    pub stdin: Vec<u8>,
    pub path_items: &'a HashMap<String, PathBuf>,
    pub path_extensions: &'a Vec<String>,
}

/// Result from a builtin command
///
/// Can report actions to execute, such as updating the theme or exiting
pub enum CommandResult {
    /// Default/OK state, means command executed sucessfully and nothing needs to be done
    Lovely,
    /// Means the command was `exit` and the shell should close
    Exit,
    /// The command requests to update the theme. The usize is the theme index
    UpdateTheme(usize),
    /// Input was not a builtin command
    NotACommand,
}

/// Recursively copy a directory
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
