use crossterm::{
    cursor::{MoveLeft, MoveRight},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    queue,
    style::{Color, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::{
    collections::VecDeque,
    io::{stdout, Result, Write},
    path::PathBuf,
    process,
};
mod colors;
mod commands;

struct Shoe {
    running: bool,
    listening: bool,
    cwd: PathBuf,
    input_text: String,
    cursor_pos: usize,
}

/// Function parse line to arguments, with support for quote enclosures
///
/// Include seperators will ensure no character of text is lost
fn parse_parts(text: &str, include_seperators: bool) -> VecDeque<CommandPart> {
    // i hate this code
    // too much logic
    let mut parts = VecDeque::new();
    parts.push_back(CommandPart {
        text: String::new(),
        part_type: CommandPartType::Keyword,
    });
    let mut last_char_was_backslash = false;
    let mut in_quote = false;
    for char in text.chars() {
        let last = parts.back_mut().unwrap();

        if char == '\\' {
            if include_seperators || last_char_was_backslash {
                last.text.insert(last.text.len(), char);
                if last_char_was_backslash {
                    last_char_was_backslash = false;
                    continue;
                }
            }
            last_char_was_backslash = true;
            continue;
        }
        if char == '"' && !last_char_was_backslash && (in_quote || last.text.is_empty()) {
            in_quote = !in_quote;
            if in_quote {
                last.part_type = CommandPartType::QuotesArg;
            }
            if !include_seperators {
                continue;
            }
        }
        if char == ' ' && !in_quote {
            if include_seperators {
                last.text.insert(last.text.len(), char);
            }
            parts.push_back(CommandPart {
                text: String::new(),
                part_type: CommandPartType::RegularArg,
            });
            last_char_was_backslash = false;
            continue;
        }
        last.text.insert(last.text.len(), char);
        last_char_was_backslash = false;
    }
    parts
}

/// Replace substring in string (non case sensitive!!)
fn replace_case_insensitive(source: String, pattern: String, replace: String) -> String {
    let mut pattern_index = 0;
    let mut found_index = None;
    for (i, c) in source.chars().enumerate() {
        if pattern_index >= pattern.chars().count() {
            break;
        }
        if c.to_lowercase().collect::<String>()
            == pattern
                .chars()
                .nth(pattern_index)
                .unwrap()
                .to_lowercase()
                .collect::<String>()
        {
            if pattern_index == 0 {
                found_index = Some(i);
            }
            pattern_index += 1;
        } else if i == source.chars().count() - 1 || found_index.is_some() {
            found_index = None;
            pattern_index = 0;
        }
        if i == source.chars().count() - 1 && pattern_index < pattern.chars().count() {
            found_index = None;
            pattern_index = 0;
        }
    }
    match found_index {
        Some(index) => {
            let mut new = String::new();
            for (i, c) in source.chars().enumerate() {
                if i < index || i >= index + pattern.chars().count() {
                    new.insert(new.len(), c);
                } else if i == index {
                    new.insert_str(new.len(), &replace);
                }
            }
            new
        }
        None => source,
    }
}

impl Shoe {
    fn new() -> Result<Self> {
        Ok(Shoe {
            running: false,
            listening: false,
            cwd: std::env::current_dir()?,
            input_text: String::new(),
            cursor_pos: 0,
        })
    }
    /// Convert cwd to a string, also replacing home path with ~
    fn cwd_to_str(&self) -> Result<String> {
        let path = self
            .cwd
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
            .to_string();
        let home_path = shellexpand::tilde("~").to_string();
        Ok(replace_case_insensitive(path, home_path, "~".to_string()))
    }
    fn handle_command(&mut self, mut parts: VecDeque<CommandPart>) -> Result<()> {
        let keyword = parts.pop_front();
        if let Some(keyword) = keyword {
            let keyword_text = keyword.text;
            let args: Vec<&String> = parts.iter().map(|item| &item.text).collect();
            let result = commands::execute_command(&keyword_text, &args);
            match result {
                commands::CommandResult::NotACommand => {}
                commands::CommandResult::Exit => {
                    self.listening = false;
                    self.running = false;
                }
                commands::CommandResult::UpdateCwd => {
                    self.cwd = std::env::current_dir()?;
                }
                _ => {}
            }
            if !matches!(result, commands::CommandResult::NotACommand) {
                queue!(stdout(), SetForegroundColor(Color::Reset))?;
                return Ok(());
            }
            let mut command = process::Command::new(keyword_text);
            command.args(args);
            let process = command.spawn();
            match process {
                Ok(mut process) => {
                    process.wait()?;
                }
                Err(_) => {
                    queue!(stdout(), SetForegroundColor(colors::ERR_COLOR))?;
                    println!("file not found! :(");
                }
            }
        }
        queue!(stdout(), SetForegroundColor(Color::Reset))?;
        Ok(())
    }
    fn write_char(&mut self, new_char: char) {
        if self.input_text.chars().count() == self.cursor_pos {
            self.input_text.insert(self.input_text.len(), new_char);
            return;
        } else if self.cursor_pos == 0 {
            self.input_text.insert(0, new_char);
            return;
        }
        let mut new = String::new();
        for (index, char) in self.input_text.chars().enumerate() {
            new.insert(new.len(), char);
            if index == self.cursor_pos - 1 {
                new.insert(new.len(), new_char);
            }
        }
        self.input_text = new;
    }
    fn delete_char(&mut self) {
        let mut new = String::new();
        for (index, char) in self.input_text.chars().enumerate() {
            if index != self.cursor_pos {
                new.insert(new.len(), char);
            }
        }
        self.input_text = new;
    }
    fn handle_key_press(&mut self, event: Event) -> Result<()> {
        if let Event::Key(key_event) = event {
            if key_event.kind != KeyEventKind::Press {
                return Ok(());
            }
            match key_event.code {
                KeyCode::Enter => {
                    self.listening = false;
                }
                KeyCode::Char(char) => {
                    if key_event.modifiers.contains(KeyModifiers::CONTROL) && char == 'c' {
                        self.listening = false;
                    } else {
                        self.write_char(char);
                        self.cursor_pos += 1;
                    }
                }
                KeyCode::Delete => {
                    self.delete_char();
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.delete_char();
                    }
                }
                KeyCode::Right => {
                    self.cursor_pos += 1;
                    if self.cursor_pos > self.input_text.chars().count() {
                        self.cursor_pos = self.input_text.chars().count();
                    }
                }
                KeyCode::Left => {
                    self.cursor_pos = self.cursor_pos.saturating_sub(1);
                }
                KeyCode::Home => {
                    self.cursor_pos = 0;
                }
                KeyCode::End => {
                    self.cursor_pos = self.input_text.chars().count();
                }
                _ => {}
            }
            self.update()?;
        }
        Ok(())
    }
    /// Prints current inputted text with color highlighting
    fn print_text(&self) -> Result<()> {
        let parts = parse_parts(&self.input_text, true);
        for part in parts {
            let color = match part.part_type {
                CommandPartType::Keyword => colors::PRIMARY_COLOR,
                CommandPartType::QuotesArg => colors::SECONDARY_COLOR,
                CommandPartType::RegularArg => Color::White,
                CommandPartType::_Special => Color::White,
            };
            queue!(stdout(), SetForegroundColor(color))?;
            print!("{}", part.text);
            queue!(stdout(), SetForegroundColor(Color::Reset))?;
        }
        Ok(())
    }
    fn update(&self) -> Result<()> {
        queue!(stdout(), Clear(ClearType::UntilNewLine))?;
        self.print_text()?;
        if self.input_text.chars().count() != 0 {
            queue!(stdout(), MoveLeft((self.input_text.chars().count()) as u16))?;
        }

        if self.cursor_pos != 0 {
            queue!(stdout(), MoveRight(self.cursor_pos as u16))?;
        }

        stdout().flush()?;

        if self.cursor_pos != 0 {
            queue!(stdout(), MoveLeft(self.cursor_pos as u16))?;
        }

        Ok(())
    }
    fn start(&mut self) -> Result<()> {
        self.running = true;
        while self.running {
            let command = &self.listen()?;
            if command.is_empty() {
                continue;
            }
            let command = parse_parts(command, false);
            self.handle_command(command)?;
        }
        Ok(())
    }
    fn listen(&mut self) -> Result<String> {
        enable_raw_mode()?;
        self.listening = true;

        queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
        print!("[");
        queue!(stdout(), SetForegroundColor(Color::White))?;
        print!("{}", self.cwd_to_str()?);
        queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR))?;
        print!("]> ");

        stdout().flush()?;
        while self.listening {
            self.handle_key_press(event::read()?)?;
        }
        if self.input_text.chars().count() != 0 {
            queue!(stdout(), MoveRight(self.input_text.chars().count() as u16))?;
        }
        println!();
        disable_raw_mode()?;
        let text = self.input_text.clone();
        self.input_text = String::new();
        self.cursor_pos = 0;
        Ok(text)
    }
}

enum CommandPartType {
    Keyword,
    QuotesArg,
    RegularArg,
    _Special,
}
struct CommandPart {
    text: String,
    part_type: CommandPartType,
}

fn main() {
    queue!(stdout(), SetForegroundColor(colors::SECONDARY_COLOR)).unwrap();
    print!("shoe ");
    queue!(stdout(), SetForegroundColor(Color::White)).unwrap();
    print!("[v{}]\n\n", env!("CARGO_PKG_VERSION"));
    stdout().flush().unwrap();

    let mut shoe = Shoe::new().unwrap();
    shoe.start().unwrap();
}
