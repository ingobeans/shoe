use commands::CommandContext;
use crossterm::{
    cursor::{MoveLeft, MoveRight},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    queue,
    style::{Color, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use relative_path::RelativePathBuf;
use std::{
    collections::VecDeque,
    env,
    io::{stdout, Read, Result, Write},
    path::{Path, PathBuf},
    process::{self, Stdio},
};
mod commands;
mod consts;

/// Function parse line to arguments, with support for quote enclosures
///
/// Include seperators will ensure no character of text is lost
fn parse_parts(text: &str, include_seperators: bool) -> VecDeque<CommandPart> {
    // i hate this code
    // too much logic
    let mut parts = VecDeque::new();
    parts.push_back(CommandPart {
        text: String::new(),
        part_type: CommandPartType::RegularArg,
    });
    let mut last_char_was_backslash = false;
    let mut in_quote = false;
    for char in text.chars() {
        let last = parts.back_mut().unwrap();

        if char == '\\' {
            if include_seperators || last_char_was_backslash {
                last.text.insert(last.text.len(), char);
            }
            last_char_was_backslash = !last_char_was_backslash;
            continue;
        }
        if char == '"' && !last_char_was_backslash && (in_quote || last.text.is_empty()) {
            in_quote = !in_quote;
            if in_quote {
                last.part_type = CommandPartType::QuotesArg;
            }
            if !include_seperators {
                last_char_was_backslash = false;
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
        if (char == ';' || char == '|' || char == '>' || char == '&' || char == '<')
            && !in_quote
            && !last_char_was_backslash
        {
            last_char_was_backslash = false;
            if !matches!(last.part_type, CommandPartType::Special) && !last.text.is_empty() {
                parts.push_back(CommandPart {
                    text: String::from(char),
                    part_type: CommandPartType::Special,
                });
                continue;
            }
            last.part_type = CommandPartType::Special;
            last.text.insert(last.text.len(), char);
            continue;
        }
        if matches!(last.part_type, CommandPartType::Special) {
            parts.push_back(CommandPart {
                text: String::from(char),
                part_type: CommandPartType::RegularArg,
            });
            last_char_was_backslash = false;
            continue;
        }
        last.text.insert(last.text.len(), char);
        last_char_was_backslash = false;
    }
    let mut make_keyword = true;
    for part in parts.iter_mut() {
        if make_keyword
            && matches!(part.part_type, CommandPartType::RegularArg)
            && !part.text.trim().is_empty()
        {
            make_keyword = false;
            part.part_type = CommandPartType::Keyword;
        } else if matches!(part.part_type, CommandPartType::Special) {
            make_keyword = true;
        }
    }
    parts
}

fn write_file<P, C>(path: P, contents: C, append: bool) -> std::io::Result<()>
where
    P: AsRef<Path>,
    C: AsRef<[u8]> + Into<Vec<u8>>,
{
    if !append {
        std::fs::write(path, contents)
    } else {
        let mut file = {
            let open = std::fs::File::open(&path);
            if let Ok(file) = open {
                file
            } else {
                std::fs::File::create(&path)?
            }
        };
        let mut file_contents = Vec::new();
        // read old contents
        file.read_to_end(&mut file_contents)?;

        let source_ends_with_newline = file_contents.ends_with(b"\n") || file_contents.is_empty();
        let new_starts_with_newline = contents.as_ref()[0] == b'\n';

        // if no newline to seperate the new and old contents, add one
        if !source_ends_with_newline && !new_starts_with_newline {
            file_contents.push(b'\n');
        }
        // add new contents
        file_contents.append(&mut contents.into());

        // write new file_contents
        std::fs::write(path, file_contents)
    }
}
fn remove_empty_parts(parts: VecDeque<CommandPart>) -> VecDeque<CommandPart> {
    let mut new = VecDeque::new();
    for part in parts {
        if part.text.trim().is_empty() && !matches!(part.part_type, CommandPartType::QuotesArg) {
        } else {
            new.push_back(part);
        }
    }
    new
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

fn list_dir(dir: &Path) -> Result<Vec<(bool, String)>> {
    let contents = std::fs::read_dir(dir)?;
    let contents = contents
        .flatten()
        .map(|item| {
            (
                item.file_type().unwrap().is_dir(),
                item.file_name().to_string_lossy().to_string(),
            )
        })
        .collect();
    Ok(contents)
}

enum AbsoluteOrRelativePathBuf {
    Relative(RelativePathBuf),
    Absolute(PathBuf),
}

/// Autocomplete an input word to a relative path
fn autocomplete(
    current_word: &String,
    mut item_index: usize,
) -> Option<(bool, AbsoluteOrRelativePathBuf)> {
    let mut valid: Vec<(bool, AbsoluteOrRelativePathBuf)> = Vec::new();
    // check if is absolute
    let path = PathBuf::from(&current_word);
    if path.is_absolute() {
        let mut file_name = path.file_name()?.to_string_lossy().to_string();
        let absolute_parent = path.parent()?;
        let contents = list_dir(absolute_parent).ok()?;

        if env::consts::OS == "windows" {
            file_name = file_name.to_lowercase()
        };
        for (is_dir, item) in contents {
            let item_in_maybe_lowercase = if env::consts::OS == "windows" {
                item.to_lowercase()
            } else {
                item.clone()
            };
            let real_item = absolute_parent.join(item);
            if item_in_maybe_lowercase.starts_with(&file_name) {
                valid.push((is_dir, AbsoluteOrRelativePathBuf::Absolute(real_item)));
            }
        }
    } else {
        let path;
        let cwd;
        if current_word.starts_with("~/") {
            let stripped_current_word = current_word[1..].to_string();
            path = RelativePathBuf::from(stripped_current_word);
            cwd = shellexpand::tilde("~").to_string().into();
        } else {
            path = RelativePathBuf::from(current_word);
            cwd = std::env::current_dir().ok()?;
        };
        let file_name = if env::consts::OS == "windows" {
            path.file_name()?.to_lowercase()
        } else {
            path.file_name()?.to_string()
        };

        let absolute = &path.to_logical_path(&cwd);
        let absolute_parent = absolute.parent()?;

        let relative_parent = path.parent()?;

        let contents = list_dir(absolute_parent).ok()?;
        for (is_dir, mut item) in contents {
            let item_in_maybe_lowercase = if env::consts::OS == "windows" {
                item.to_lowercase()
            } else {
                item.clone()
            };
            if item_in_maybe_lowercase.starts_with(&file_name) {
                if current_word.starts_with("~/") {
                    item = "~/".to_string() + &item;
                }
                let real_item = relative_parent.join(item);
                valid.push((is_dir, AbsoluteOrRelativePathBuf::Relative(real_item)));
            }
        }
    }
    if !valid.is_empty() {
        item_index %= valid.len();
        for (index, item) in valid.into_iter().enumerate() {
            if index == item_index {
                return Some(item);
            }
        }
    }

    None
}

fn absolute_pathbuf_to_string(input: PathBuf) -> String {
    let mut parts: Vec<String> = vec![];
    for component in input.components() {
        match component {
            std::path::Component::Normal(path) => {
                parts.push(path.to_string_lossy().to_string());
            }
            std::path::Component::Prefix(prefix_component) => {
                parts.push(prefix_component.as_os_str().to_string_lossy().to_string());
            }
            _ => {}
        };
    }
    parts.join("/")
}

enum CommandInputModifier {
    /// Read command input from file
    ReadFrom(String),
    /// Command input has no modifier.
    Default,
}
enum CommandOutputModifier {
    /// Command output has been piped.
    Piped,
    /// Command output has been redirected to a file. (path,append)
    WriteTo(String, bool),
    /// Command output has no modifier.
    Default,
}

enum RunCondition {
    Any,
    Success,
    Fail,
}

struct Command<'a> {
    keyword: String,
    args: VecDeque<&'a str>,
    output_modifier: CommandOutputModifier,
    input_modifier: CommandInputModifier,
    run_condition: RunCondition,
}
struct Shoe {
    history_path: Option<String>,
    history: Vec<String>,
    history_index: usize,
    running: bool,
    listening: bool,
    cwd: PathBuf,
    input_text: String,
    cursor_pos: usize,
    current_dir_contents: Vec<String>,
    autocomplete_cycle_index: Option<usize>,
    last_input_before_autocomplete: Option<String>,
}

impl Shoe {
    fn new(history_path: Option<String>, rc: Vec<String>) -> Result<Self> {
        let history: Vec<String>;
        if let Some(history_path) = &history_path {
            let history_text =
                std::fs::read_to_string(&history_path).expect("Couldn't read ~/.shoehistory");
            history = history_text
                .split('\n')
                .filter_map(|line| {
                    if line.trim().is_empty() {
                        None
                    } else {
                        Option::<String>::Some(line.to_string())
                    }
                })
                .collect();
        } else {
            history = Vec::new();
        }
        let history_index = history.len();

        let cwd = std::env::current_dir()?;
        let current_dir_contents = list_dir(&cwd)?.into_iter().map(|f| f.1).collect();

        let mut instance = Shoe {
            history_path,
            history,
            history_index,
            running: false,
            listening: false,
            cwd,
            input_text: String::new(),
            cursor_pos: 0,
            current_dir_contents,
            last_input_before_autocomplete: None,
            autocomplete_cycle_index: None,
        };

        for command in rc {
            instance.execute_command(&command, false)?;
        }

        Ok(instance)
    }
    /// Convert cwd to a string, also replacing home path with ~
    fn cwd_to_str(&self) -> Result<String> {
        let path = self
            .cwd
            .to_str()
            .ok_or(std::io::Error::other("Couldn't read path as string"))?
            .to_string();
        let home_path = shellexpand::tilde("~").to_string();

        if env::consts::OS == "windows" {
            // windows has case insensitive paths
            Ok(replace_case_insensitive(path, home_path, "~".to_string()))
        } else {
            Ok(path.replace(&home_path, "~"))
        }
    }
    fn update_cwd(&mut self) -> Result<()> {
        self.cwd = std::env::current_dir()?;
        self.current_dir_contents = list_dir(&self.cwd)?.into_iter().map(|f| f.1).collect();
        Ok(())
    }
    fn execute_commands(&mut self, commands: Vec<Command>) -> Result<()> {
        // if command has output piped to the next, store the output here
        let mut last_piped_output: Option<Vec<u8>> = None;
        // if last command succeeeded
        let mut last_success: Option<bool> = None;

        // run each command sequentially
        for command in &commands {
            queue!(stdout(), SetForegroundColor(Color::Reset))?;
            // check commands run condition
            // ex. for when doing `echo "do something which may fail" && echo it succeeded!`
            match command.run_condition {
                RunCondition::Success => {
                    if let Some(last_success) = last_success {
                        if !last_success {
                            continue;
                        }
                    }
                }
                RunCondition::Fail => {
                    if let Some(last_success) = last_success {
                        if last_success {
                            continue;
                        }
                    }
                }
                RunCondition::Any => {}
            }
            // what to use as stdin (if none, inherit)
            let mut stdin_data: Option<Vec<u8>> = None;

            // if there was output piped from last command, set stdin_data to that
            if let Some(buf) = last_piped_output {
                stdin_data = Some(buf);
                last_piped_output = None;
            } else {
                // otherwise, handle input modifiers
                match &command.input_modifier {
                    // if stdin is derived from file contents
                    CommandInputModifier::ReadFrom(path) => {
                        stdin_data = Some(std::fs::read(path)?);
                    }
                    _ => {}
                }
            }

            // what got outputed, (if none, it was written directly to inherited stdout)
            let mut stdout_data: Option<Vec<u8>> = None;

            // try running built in command
            let mut output_buf = Vec::new();
            let mut context = CommandContext {
                args: &command.args,
                stdout: &mut output_buf,
            };
            let result = commands::execute_command(&command.keyword, &mut context);
            let mut not_a_builtin_command = false;

            last_success = Some(true);
            match result {
                commands::CommandResult::Error => {
                    last_success = Some(false);
                }
                commands::CommandResult::Exit => {
                    self.listening = false;
                    self.running = false;
                    return Ok(());
                }
                commands::CommandResult::UpdateCwd => self.update_cwd()?,
                commands::CommandResult::Lovely => {
                    if let CommandOutputModifier::Default = command.output_modifier {
                        // write output
                        stdout().lock().write_all(&output_buf)?;
                    }
                    let stripped_output = strip_ansi_escapes::strip(output_buf);
                    stdout_data = Some(stripped_output);
                }
                commands::CommandResult::NotACommand => {
                    not_a_builtin_command = true;
                }
            }

            // if command isnt a builtin, run process
            if not_a_builtin_command {
                // if on windows, also try running the keyword with the .bat and .cmd extensions if the regular fails
                let keywords: Vec<String>;
                let mut any_worked = false;
                if env::consts::OS == "windows" {
                    if !command.keyword.contains(".") {
                        keywords = vec![
                            command.keyword.to_string(),
                            command.keyword.to_string() + ".bat",
                            command.keyword.to_string() + ".cmd",
                        ]
                    } else {
                        keywords = vec![command.keyword.to_string()];
                    }
                } else {
                    keywords = vec![command.keyword.to_string()];
                }

                for keyword in keywords {
                    let mut process = process::Command::new(&keyword);
                    process.args(&command.args);

                    // if there's stdin data, set process' stdin to piped
                    if stdin_data.is_some() {
                        process.stdin(Stdio::piped());
                    }

                    // if the command's output modifier is not default, set process' stdout to be piped (so we can handle it later)
                    if !matches!(command.output_modifier, CommandOutputModifier::Default) {
                        process.stdout(Stdio::piped());
                    }

                    // if process cant be spawned (path doesnt exist), continue
                    // on windows this will mean testing the next path (since it tests both the path, the path + ".bat" and the path + ".cmd")
                    // otherwise it will just move on to the next command
                    let Ok(mut process) = process.spawn() else {
                        continue;
                    };

                    // process was created successfully
                    any_worked = true;

                    if let Some(buf) = stdin_data {
                        if let Some(stdin) = &mut process.stdin {
                            stdin.write_all(&buf)?;
                        }
                    }
                    let success = process.wait()?.success();

                    // if process has readable stdout, strip it from ansi codes and store here
                    let stripped_output: Option<Vec<u8>> = if let Some(stdout) = &mut process.stdout
                    {
                        let mut buf: Vec<u8> = Vec::new();
                        stdout.read_to_end(&mut buf)?;
                        let stripped = strip_ansi_escapes::strip(buf);
                        Some(stripped)
                    } else {
                        None
                    };

                    // set success state to the process' success state (its error code being 0)
                    last_success = Some(success);

                    stdout_data = stripped_output;
                    break;
                }
                if !any_worked {
                    last_success = Some(false);
                    queue!(stdout(), SetForegroundColor(consts::ERR_COLOR)).unwrap();
                    println!("file/command '{}' not found! :(", command.keyword);
                }
            }

            // handle command output
            match &command.output_modifier {
                CommandOutputModifier::Piped => {
                    last_piped_output = stdout_data;
                }
                CommandOutputModifier::WriteTo(path, append) => {
                    if let Some(stdout_data) = stdout_data {
                        write_file(path, stdout_data, *append)?;
                    }
                }
                _ => {}
            }
        }
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
    fn get_word_at_cursor(&self) -> Option<(usize, CommandPart)> {
        let mut counter = 0;
        for (index, part) in parse_parts(&self.input_text, false).into_iter().enumerate() {
            counter += part.text.chars().count() + 1;
            if counter >= self.cursor_pos {
                return Some((index, part));
            }
        }
        None
    }
    fn handle_key_press(&mut self, event: Event) -> Result<()> {
        if let Event::Key(key_event) = event {
            if key_event.kind != KeyEventKind::Press {
                return Ok(());
            }
            let mut reset_autocomplete_cycle = true;
            match key_event.code {
                KeyCode::Enter => {
                    self.listening = false;
                }
                KeyCode::Char(char) => {
                    if key_event.modifiers.contains(KeyModifiers::CONTROL) && char == 'c' {
                        self.input_text = String::new();
                        self.listening = false;
                    } else {
                        self.write_char(char);
                        self.cursor_pos += 1;
                    }
                }
                KeyCode::Tab => 'tab: {
                    reset_autocomplete_cycle = false;
                    if self.input_text.is_empty() {
                        break 'tab;
                    }
                    if let Some(last_input) = &self.last_input_before_autocomplete {
                        self.cursor_pos -= self.input_text.len().saturating_sub(last_input.len());
                        self.input_text = last_input.to_string();
                        self.autocomplete_cycle_index =
                            Some(self.autocomplete_cycle_index.unwrap() + 1);
                    } else {
                        self.autocomplete_cycle_index = Some(0);
                        self.last_input_before_autocomplete = Some(self.input_text.to_string());
                    }
                    let mut words = parse_parts(&self.input_text, true);
                    let Some((word_index, word)) = self.get_word_at_cursor() else {
                        break 'tab;
                    };
                    let ends_with_quote =
                        matches!(words[word_index].part_type, CommandPartType::QuotesArg)
                            && words[word_index].text.ends_with('"');
                    let starts_with_quote =
                        matches!(words[word_index].part_type, CommandPartType::QuotesArg);
                    let ends_with_space = words[word_index].text.ends_with(' ');
                    words.remove(word_index);

                    let result = autocomplete(&word.text, self.autocomplete_cycle_index.unwrap());
                    let Some((autocompletion_is_dir, autocompletion)) = result else {
                        break 'tab;
                    };
                    let mut autocompletion_string: String;
                    match autocompletion {
                        AbsoluteOrRelativePathBuf::Relative(relative) => {
                            autocompletion_string = relative.to_string();
                        }
                        AbsoluteOrRelativePathBuf::Absolute(absolute) => {
                            autocompletion_string = absolute_pathbuf_to_string(absolute);
                        }
                    }
                    if autocompletion_is_dir {
                        autocompletion_string += "/";
                    }
                    if autocompletion_string.contains(' ') && !starts_with_quote {
                        autocompletion_string = String::from("\"") + &autocompletion_string;
                        if word_index != words.len() {
                            autocompletion_string += "\"";
                        }
                    }
                    self.cursor_pos +=
                        autocompletion_string.chars().count() - word.text.chars().count();
                    if starts_with_quote {
                        autocompletion_string = String::from("\"") + &autocompletion_string;
                    }
                    if ends_with_quote {
                        autocompletion_string += "\"";
                    }
                    if ends_with_space {
                        autocompletion_string += " ";
                    }
                    let mut new = String::new();
                    for (index, word) in words.iter().enumerate() {
                        if word_index == index {
                            new += &autocompletion_string;
                        }
                        new += &word.text;
                    }
                    if word_index == words.len() {
                        new += &autocompletion_string;
                    }
                    self.input_text = new;
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
                KeyCode::Up => {
                    if !self.history.is_empty() {
                        if self.history_index > 0 {
                            self.history_index -= 1;
                        }
                        self.input_text = self.history[self.history_index].clone();
                        self.cursor_pos = self.input_text.chars().count();
                    }
                }
                KeyCode::Down => {
                    if !self.history.is_empty() {
                        if self.history_index < self.history.len() {
                            self.history_index += 1;
                        }
                        if self.history_index < self.history.len() {
                            self.input_text = self.history[self.history_index].clone();
                            self.cursor_pos = self.input_text.chars().count();
                        } else {
                            self.input_text = String::new();
                            self.cursor_pos = 0;
                        }
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
            if reset_autocomplete_cycle {
                self.autocomplete_cycle_index = None;
                self.last_input_before_autocomplete = None;
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
                CommandPartType::Keyword => consts::PRIMARY_COLOR,
                CommandPartType::QuotesArg => consts::SECONDARY_COLOR,
                CommandPartType::RegularArg => Color::White,
                CommandPartType::Special => consts::SECONDARY_COLOR,
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
    fn parts_to_commands_vec(parts: &VecDeque<CommandPart>) -> Result<Vec<Command>> {
        let mut commands: Vec<Command> = Vec::new();
        let mut current_command: Option<Command> = None;
        let mut index = 0;
        let mut next_run_condition = RunCondition::Any;

        while index < parts.len() {
            let part = &parts[index];
            index += 1;
            let mut done = false;
            if let Some(command) = &mut current_command {
                if let CommandPartType::Special = part.part_type {
                    match part.text.as_str() {
                        ";" | "&" => {
                            done = true;
                        }
                        "&&" => {
                            done = true;
                            next_run_condition = RunCondition::Success;
                        }
                        "||" => {
                            done = true;
                            next_run_condition = RunCondition::Fail;
                        }
                        "|" => {
                            done = true;
                            command.output_modifier = CommandOutputModifier::Piped;
                        }
                        ">" => {
                            command.output_modifier =
                                CommandOutputModifier::WriteTo(parts[index].text.clone(), false);
                            index += 1;
                        }
                        ">>" => {
                            command.output_modifier =
                                CommandOutputModifier::WriteTo(parts[index].text.clone(), true);
                            index += 1;
                        }
                        "<" => {
                            command.input_modifier =
                                CommandInputModifier::ReadFrom(parts[index].text.clone());
                            index += 1;
                        }
                        _ => {
                            let message = format!("unexpected token '{}'", part.text);
                            return Err(std::io::Error::other(message));
                        }
                    }
                } else {
                    command.args.push_back(&part.text);
                }
            } else {
                current_command = Some(Command {
                    keyword: part.text.clone(),
                    args: VecDeque::new(),
                    output_modifier: CommandOutputModifier::Default,
                    input_modifier: CommandInputModifier::Default,
                    run_condition: next_run_condition,
                });
                next_run_condition = RunCondition::Any;
            }

            if done || index == parts.len() {
                if let Some(command) = current_command {
                    commands.push(command);
                    current_command = None;
                }
            }
        }
        Ok(commands)
    }
    fn execute_command(&mut self, command: &String, allow_add_to_history: bool) -> Result<()> {
        if command.trim().is_empty() {
            self.history_index = self.history.len();
            return Ok(());
        }
        let mut should_store_history = true;
        if let Some(last_input) = self.history.last() {
            if last_input == command {
                should_store_history = false;
            }
        }
        should_store_history &= allow_add_to_history;

        if should_store_history {
            self.history.push(command.clone());

            if let Some(history_path) = &self.history_path {
                std::fs::write(history_path, self.history.join("\n"))?;
            }
        }

        self.history_index = self.history.len();

        let parts = remove_empty_parts(parse_parts(command, false));

        // store any errors that arise here
        let mut err: Option<std::io::Error> = None;

        let commands = Self::parts_to_commands_vec(&parts);
        match commands {
            Ok(commands) => {
                let execution_result = self.execute_commands(commands);

                if let Err(error) = execution_result {
                    // if command execution failed, store error in err
                    err = Some(error);
                }
            }
            Err(error) => {
                // if commands parsing failed, store error in err
                err = Some(error);
            }
        }

        if let Some(err) = err {
            let _ = queue!(stdout(), SetForegroundColor(consts::ERR_COLOR));
            println!("{}", err);
        }

        queue!(stdout(), SetForegroundColor(Color::Reset))?;
        Ok(())
    }
    fn start(&mut self) -> Result<()> {
        self.running = true;
        while self.running {
            let command = &self.listen()?;
            self.execute_command(command, true)?;
        }
        Ok(())
    }
    fn listen(&mut self) -> Result<String> {
        enable_raw_mode()?;
        self.listening = true;

        queue!(stdout(), SetForegroundColor(consts::PRIMARY_COLOR))?;
        print!("[");
        queue!(stdout(), SetForegroundColor(Color::White))?;
        print!("{}", self.cwd_to_str()?);
        queue!(stdout(), SetForegroundColor(consts::PRIMARY_COLOR))?;
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
    Special,
}
struct CommandPart {
    text: String,
    part_type: CommandPartType,
}

fn main() {
    let args = std::env::args();
    let mut use_history = true;
    let mut use_rc = true;
    let mut first = true;
    for arg in args {
        if first {
            first = false;
            continue;
        }
        match arg.as_str() {
            "--no-history" => {
                use_history = false;
            }
            "--no-rc" => {
                use_rc = false;
            }
            "-h" | "--help" => {
                println!("{}", consts::HELP_MESSAGE);
                return;
            }
            _ => {
                queue!(stdout(), SetForegroundColor(consts::ERR_COLOR)).unwrap();
                println!("unknown arg: '{}'. do -h for help", arg);
                queue!(stdout(), SetForegroundColor(Color::Reset)).unwrap();
                return;
            }
        }
    }

    queue!(stdout(), SetForegroundColor(consts::PRIMARY_COLOR)).unwrap();
    print!("shoe ");
    queue!(stdout(), SetForegroundColor(Color::White)).unwrap();
    print!("[v{}]\n\n", env!("CARGO_PKG_VERSION"));
    stdout().flush().unwrap();

    let path: Option<String>;
    if use_history {
        let history_path = shellexpand::tilde("~/.shoehistory").to_string();
        if std::fs::metadata(&history_path).is_err() {
            std::fs::write(&history_path, "").expect("Couldn't create ~/.shoehistory");
        }
        path = Some(history_path);
    } else {
        path = None;
    }

    let rc: Vec<String>;
    if use_rc {
        let rc_path = shellexpand::tilde("~/.shoerc").to_string();
        if std::fs::metadata(&rc_path).is_err() {
            std::fs::write(&rc_path, "").expect("Couldn't create ~/.shoerc");
        }
        rc = std::fs::read_to_string(&rc_path)
            .expect("Couldn't read ~/.shoerc")
            .split('\n')
            .map(str::to_string)
            .collect()
    } else {
        rc = Vec::new();
    }

    let mut shoe = Shoe::new(path, rc).unwrap();
    shoe.start().unwrap();
}
