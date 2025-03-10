use commands::CommandContext;
use crossterm::{
    cursor::{MoveDown, MoveRight, MoveToColumn, MoveUp},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    queue,
    style::{Color, SetAttribute, SetForegroundColor},
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
use utils::{Theme, THEMES};
mod commands;
mod utils;

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

        // set `last_char_was_backslash` to false
        // while storing the original value in new `last_was_backslash`
        let last_was_backslash = last_char_was_backslash;
        last_char_was_backslash = false;

        match char {
            '\\' => {
                if include_seperators || last_was_backslash {
                    last.text.insert(last.text.len(), char);
                }
                last_char_was_backslash = !last_was_backslash;
                continue;
            }
            '"' if !last_was_backslash && (in_quote || last.text.is_empty()) => {
                in_quote = !in_quote;
                if include_seperators {
                    last.text.insert(last.text.len(), char);
                }
                if in_quote {
                    last.part_type = CommandPartType::QuotesArg;
                } else {
                    parts.push_back(CommandPart {
                        text: String::new(),
                        part_type: CommandPartType::RegularArg,
                    });
                }
                continue;
            }
            ' ' if !in_quote => {
                if include_seperators {
                    last.text.insert(last.text.len(), char);
                }
                parts.push_back(CommandPart {
                    text: String::new(),
                    part_type: CommandPartType::RegularArg,
                });
                continue;
            }
            ';' | '|' | '>' | '&' | '<' if !in_quote && !last_was_backslash => {
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
            _ => {}
        }
        if let CommandPartType::Special = last.part_type {
            parts.push_back(CommandPart {
                text: String::from(char),
                part_type: CommandPartType::RegularArg,
            });
            continue;
        }
        last.text.insert(last.text.len(), char);
    }
    // make first non empty regular arg after each seperator a keyword
    let mut make_keyword = true;
    for part in parts.iter_mut() {
        match part.part_type {
            CommandPartType::RegularArg => {
                if make_keyword && !part.text.trim().is_empty() {
                    make_keyword = false;
                    part.part_type = CommandPartType::Keyword;
                }
            }
            CommandPartType::Special => {
                make_keyword = true;
            }
            _ => {}
        }
    }
    // return parts
    parts
}
fn move_cursor_to_cursor_pos(original_x: usize, pos: usize, width: usize) -> Result<()> {
    let new_x = (original_x + pos) % width;
    let rows = (original_x + pos) / width;
    if rows > 0 {
        queue!(stdout(), MoveDown(rows as u16))?;
    }

    queue!(stdout(), MoveToColumn(new_x as u16))?;
    Ok(())
}

fn move_back_to(original_x: usize, steps: usize, width: usize) -> Result<()> {
    let rows = (original_x + steps) / width;
    if rows > 0 {
        queue!(stdout(), MoveUp(rows as u16))?;
    }
    queue!(stdout(), MoveToColumn(original_x as u16))?;
    Ok(())
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
struct Shoe<'a> {
    history_path: Option<String>,
    history: Vec<String>,
    history_index: usize,
    theme: &'a Theme<'a>,
    running: bool,
    listening: bool,
    use_suggestions: bool,
    substitute_tildes: bool,
    input_text: String,
    cursor_pos: usize,
    autocomplete_cycle_index: Option<usize>,
    last_input_before_autocomplete: Option<String>,
}

impl Shoe<'_> {
    fn new(history_path: Option<String>, rc: Vec<String>) -> Result<Self> {
        let history: Vec<String>;
        if let Some(history_path) = &history_path {
            let history_text =
                std::fs::read_to_string(history_path).expect("Couldn't read ~/.shoehistory");
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
        let is_windows = env::consts::OS == "windows";

        let mut instance = Shoe {
            history_path,
            history,
            history_index,
            theme: &THEMES[0],
            running: false,
            listening: false,
            use_suggestions: true,
            substitute_tildes: is_windows,
            input_text: String::new(),
            cursor_pos: 0,
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
        let path = std::env::current_dir()?
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
    fn generate_keyword_variants(&self, keyword: &String) -> Vec<String> {
        // necessary function to ensure you can run files from their path or name if they're in the env PATH variable
        // also on windows tries with different extensions
        // (so you can type just 'example' instead of 'example.bat' or 'example.exe')
        // a lot of this code may seem random but it is to fix very specific odd behaviours of windows
        // but i tried to comment the specifics as much as possible
        let is_path = keyword.contains('/') || keyword.contains('\\');
        let keyword_has_file_extension = PathBuf::from(&keyword).extension().is_some();

        let mut path_variants: Vec<String> = vec![keyword.to_string()];

        // on windows try the keyword with more extensions
        if env::consts::OS == "windows" && !keyword_has_file_extension {
            path_variants.push(keyword.to_string() + ".exe");
            path_variants.push(keyword.to_string() + ".bat");
            path_variants.push(keyword.to_string() + ".cmd");
        }

        let cwd = std::env::current_dir();
        if let Ok(cwd) = cwd {
            for variant in &path_variants {
                // check if a file with keyword as path exists relative to cwd
                let variant_as_pathbuf = RelativePathBuf::from(&variant);
                let real_path = variant_as_pathbuf.to_logical_path(&cwd);
                if real_path.is_file() {
                    // if the keyword is a path that does exist (possibly with the extra file extensions)
                    // return the path directly, to ensure windows can find it
                    // this fixes so you dont have to type './example' and can just do 'example'
                    return vec![real_path.to_string_lossy().to_string()];
                }
            }
        }

        // if not keyword is a direct path to an executable

        if !is_path {
            // return variants with ".exe", ".bat" and ".cmd" extensions if keyword doesnt appear to be a path, i.e. you're probably running something by name from env PATH
            // the reason for returning multiple variants here is that if a batch file is added to path, you'd otherwise have to type 'name.bat'
            // but this ensures you only need 'name'.
            path_variants
        } else {
            // if keyword does appear to be a path, even though it doesnt seem to exist, return the original keyword.
            // here we dont try any other variants, since if you try running a batch file by path, that doesnt exist
            // you get an error to command line output saying "<path> is not recognized as an internal or external command,operable program or batch file."
            // instead of being caught when trying to create the process
            vec![keyword.to_string()]
        }
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
                    CommandInputModifier::Default => {}
                }
            }

            // what got outputed, (if none, it was written directly to inherited stdout)
            let mut stdout_data: Option<Vec<u8>> = None;

            // try running built in command
            let mut output_buf = Vec::new();
            let mut context = CommandContext {
                args: &command.args,
                theme: self.theme,
                stdout: &mut output_buf,
            };
            let result = commands::execute_command(&command.keyword, &mut context);
            let mut not_a_builtin_command = false;

            last_success = Some(result.is_ok());
            match result {
                Ok(result) => {
                    match result {
                        commands::CommandResult::Exit => {
                            self.listening = false;
                            self.running = false;
                            return Ok(());
                        }
                        commands::CommandResult::UpdateTheme(new_index) => {
                            self.theme = &THEMES[new_index];
                        }
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
                }
                Err(error) => {
                    queue!(stdout(), SetForegroundColor(self.theme.err_color))?;
                    println!("{}", error);
                    continue;
                }
            }

            // if command isnt a builtin, run process
            if not_a_builtin_command {
                // try multiple slight modifications of the keyword
                // in case it failes.
                // on windows, this includes trying the keyword with ".bat" and ".cmd" appended to the end (if keyword has no extension)
                let keywords = self.generate_keyword_variants(&command.keyword);

                // if any of the keywords succeeded
                let mut any_worked = false;

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

                    // if process cant be spawned (path doesnt exist), test the next keyword variant
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
                    queue!(stdout(), SetForegroundColor(self.theme.err_color)).unwrap();
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
                KeyCode::Esc => {
                    self.input_text = String::new();
                    self.cursor_pos = 0;
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
                    let part_type = &words[word_index].part_type;

                    // so we know if we need to strip before autocompletion and then re-add at the end
                    // not all QuoteArgs end with quotes, as one isnt needed, so we need to check that it actually ends with one.
                    let ends_with_quote = matches!(part_type, CommandPartType::QuotesArg)
                        && words[word_index].text.ends_with('"');
                    // all QuotesArgs will start with a quote
                    let starts_with_quote = matches!(part_type, CommandPartType::QuotesArg);

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
                        // if we press right arrow at the last character, fill in suggestion
                        if let Some(suggestion) = self.get_suggestion() {
                            self.input_text = suggestion.clone();
                        }
                        // move to last char
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
                CommandPartType::Keyword => self.theme.primary_color,
                CommandPartType::QuotesArg => self.theme.secondary_color,
                CommandPartType::RegularArg => {
                    if part.text.starts_with("-") {
                        self.theme.secondary_color
                    } else {
                        Color::White
                    }
                }
                CommandPartType::Special => self.theme.secondary_color,
            };
            queue!(stdout(), SetForegroundColor(color))?;
            print!("{}", part.text);
        }
        Ok(())
    }
    fn update(&self) -> Result<()> {
        queue!(stdout(), Clear(ClearType::FromCursorDown))?;

        self.print_text()?;
        let mut cursor_steps = self.input_text.chars().count();

        // dont show suggestion when self.listening is false, i.e. the user just pressed enter
        // so suggestions for previous entries are hidden
        let should_show_suggestion = self.listening && self.use_suggestions;

        if should_show_suggestion {
            let suggestion = self.get_suggestion();
            if let Some(suggestion) = suggestion {
                // cut suggestion to only the new part
                let cut_suggestion = &suggestion.clone()[self.input_text.len()..];
                // make text dark grey and italic
                queue!(stdout(), SetForegroundColor(Color::DarkGrey))?;
                queue!(stdout(), SetAttribute(crossterm::style::Attribute::Italic))?;
                // print suggestion
                print!("{}", cut_suggestion);
                // restore text
                queue!(
                    stdout(),
                    SetAttribute(crossterm::style::Attribute::NoItalic)
                )?;
                // increase cursor steps so cursor is properly moved back to the beginning of the line
                cursor_steps += cut_suggestion.chars().count();
            }
        }
        queue!(stdout(), SetForegroundColor(Color::Reset))?;
        let (width, _) = crossterm::terminal::size()?;
        let start_x = self.cwd_to_str()?.chars().count() + 4;
        let width = width as usize;
        if cursor_steps + start_x == width {
            print!(" ");
            cursor_steps += 1;
        }

        // move back cursor to the beginning of the prompt
        move_back_to(start_x, cursor_steps, width)?;

        // show cursor at the cursor_pos, then move back to the beginning of the prompt
        move_cursor_to_cursor_pos(start_x, self.cursor_pos, width)?;

        // render updates
        stdout().flush()?;

        // restore cursor pos once more
        move_back_to(start_x, self.cursor_pos, width)?;

        Ok(())
    }
    fn get_suggestion(&self) -> Option<&String> {
        if self.input_text.trim().is_empty() {
            return None;
        }
        for index in 0..self.history.len() {
            // get items from history in reversed order
            let item = &self.history[self.history.len() - 1 - index];

            if item.starts_with(&self.input_text) {
                return Some(item);
            }
        }
        None
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

        let mut parts = remove_empty_parts(parse_parts(command, false));

        if self.substitute_tildes {
            for part in parts.iter_mut() {
                if part.text.contains('~') {
                    let new = shellexpand::tilde(&part.text).to_string();
                    part.text = new;
                }
            }
        }

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
            let _ = queue!(stdout(), SetForegroundColor(self.theme.err_color));
            println!("{}", err);
        }

        queue!(stdout(), SetForegroundColor(Color::Reset))?;
        Ok(())
    }
    fn start(&mut self) -> Result<()> {
        // print banner
        queue!(stdout(), SetForegroundColor(self.theme.primary_color)).unwrap();
        print!("shoe ");
        queue!(stdout(), SetForegroundColor(Color::White)).unwrap();
        print!("[v{}]\n\n", env!("CARGO_PKG_VERSION"));
        stdout().flush().unwrap();

        // disable ctrl+c
        ctrlc::set_handler(|| {}).unwrap();

        // run
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

        queue!(stdout(), SetForegroundColor(self.theme.primary_color))?;
        print!("[");
        queue!(stdout(), SetForegroundColor(Color::White))?;
        print!("{}", self.cwd_to_str()?);
        queue!(stdout(), SetForegroundColor(self.theme.primary_color))?;
        print!("]> ");

        stdout().flush()?;
        while self.listening {
            self.handle_key_press(event::read()?)?;
        }
        if self.input_text.chars().count() != 0 {
            queue!(stdout(), MoveRight(self.input_text.chars().count() as u16))?;
        }
        queue!(stdout(), MoveToColumn(0))?;
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

    // will be Some if the -c or -k argument has been hit, if so, all following args are appended to this
    let mut run_command: Option<String> = None;
    let mut exit_after_run_command = false;
    for arg in args.skip(1) {
        // if -c or -k has been hit, simply append this arg to run_command
        if let Some(run_command) = &mut run_command {
            *run_command += &(arg + " ");
        } else {
            match arg.as_str() {
                "--no-history" => {
                    use_history = false;
                }
                "--no-rc" => {
                    use_rc = false;
                }
                "-h" | "--help" => {
                    println!("{}", utils::HELP_MESSAGE);
                    return;
                }
                "-c" | "--command" => {
                    run_command = Some(String::new());
                    exit_after_run_command = true;
                }
                // i didnt know what the 'k' stood for
                "-k" | "--command-but-like-dont-exit-after" => {
                    run_command = Some(String::new());
                }
                _ => {
                    queue!(stdout(), SetForegroundColor(utils::DEFAULT_ERR_COLOR)).unwrap();
                    println!("unknown arg: '{}'. do -h for help", arg);
                    queue!(stdout(), SetForegroundColor(Color::Reset)).unwrap();
                    return;
                }
            }
        }
    }

    let path: Option<String> = if use_history {
        let history_path = shellexpand::tilde("~/.shoehistory").to_string();
        if std::fs::metadata(&history_path).is_err() {
            std::fs::write(&history_path, "").expect("Couldn't create ~/.shoehistory");
        }
        Some(history_path)
    } else {
        None
    };

    let mut rc: Vec<String> = if use_rc {
        let rc_path = shellexpand::tilde("~/.shoerc").to_string();
        if std::fs::metadata(&rc_path).is_err() {
            std::fs::write(&rc_path, "").expect("Couldn't create ~/.shoerc");
        }
        std::fs::read_to_string(&rc_path)
            .expect("Couldn't read ~/.shoerc")
            .split('\n')
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };

    // if a command has been specified through -c or -k argument, add that to end of rc
    if let Some(run_command) = run_command {
        rc.push(run_command);
    }

    // construct shoe instance
    let mut shoe = Shoe::new(path, rc).unwrap();

    // if argument was -c, dont continue running shell
    if exit_after_run_command {
        return;
    }
    shoe.start().unwrap();
}
