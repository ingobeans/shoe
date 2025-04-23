//! Constants and small helper functions

use crossterm::style::Color;
use regex::Regex;

pub static DEFAULT_ERR_COLOR: Color = Color::Red;

pub static HELP_MESSAGE: &str = "
--no-history  - dont store history in ~/.shoehistory
--no-rc       - dont run startup commands from ~/.shoerc
-h            - displays this help message
-c            - run all args passed afterwards as a command, then exit
-k            - run all args passed afterwards as a command";

pub struct Theme {
    pub name: &'static str,
    pub primary_color: Color,
    pub secondary_color: Color,
    pub err_color: Color,
}
const fn hex_to_color(hex: u32) -> Color {
    Color::Rgb {
        r: ((hex >> 16) & 0xFF) as u8,
        g: ((hex >> 8) & 0xFF) as u8,
        b: ((hex) & 0xFF) as u8,
    }
}
/// Strip bytes of ansi escape codes
pub fn strip(input: Vec<u8>) -> Vec<u8> {
    let input = String::from_utf8_lossy(&input).to_string();
    strip_str(&input).as_bytes().into()
}
/// Strip ansi codes from a given string using Regex.
///
/// Stolen from [horizon](https://github.com/66HEX/horizon/blob/c21ee69c0d4c0543006a5c08523d39fc61bb0ba3/src-tauri/src/terminal.rs#L34)
pub fn strip_str(input: &str) -> String {
    lazy_static::lazy_static! {
        static ref PATTERNS: Vec<Regex> = vec![
            Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap(),
            Regex::new(r"\x1b\]8;;.*?\x1b\\").unwrap(),
            Regex::new(r"\x1b\]8;;.*?\x07").unwrap(),
            Regex::new(r"\x1b\]1337;.*?\x1b\\").unwrap(),
            Regex::new(r"\x1b\]1337;.*?\x07").unwrap(),
            Regex::new(r"\x1b\[\?25[hl]").unwrap(),
            Regex::new(r"\x1b\[[0-9]*[ABCDEFGHJKST]").unwrap(),
            Regex::new(r"\x1b\[[0-9]*[JK]").unwrap(),
            Regex::new(r"\x1b\[[0-9;]*m").unwrap(),
            Regex::new(r"\x1b\[[0-9;]*[cnsu]").unwrap(),
            Regex::new(r"\x1b\[[0-9;]*[hl]").unwrap(),
            Regex::new(r"\x1b\[[^a-zA-Z]*[a-zA-Z]").unwrap(),
            Regex::new(r"\x1b\][^a-zA-Z]*[a-zA-Z]").unwrap(),
            Regex::new(r"\x1b[^a-zA-Z]").unwrap(),
        ];
    }

    let mut result = input.to_string();
    for pattern in PATTERNS.iter() {
        result = pattern.replace_all(&result, "").to_string();
    }
    result
}
pub static THEMES: &[Theme] = &[
    Theme {
        name: "gold",
        primary_color: hex_to_color(0xFFC145),
        secondary_color: hex_to_color(0x5B5F97),
        err_color: DEFAULT_ERR_COLOR,
    },
    Theme {
        name: "earth",
        primary_color: hex_to_color(0x45FF8C),
        secondary_color: hex_to_color(0x97645B),
        err_color: DEFAULT_ERR_COLOR,
    },
    Theme {
        name: "element",
        primary_color: hex_to_color(0xFF4C4F),
        secondary_color: hex_to_color(0x89B4E5),
        err_color: DEFAULT_ERR_COLOR,
    },
    Theme {
        name: "lime",
        primary_color: hex_to_color(0x9DE64E),
        secondary_color: hex_to_color(0x72A6FF),
        err_color: DEFAULT_ERR_COLOR,
    },
    Theme {
        name: "fire",
        primary_color: hex_to_color(0xFF2B32),
        secondary_color: hex_to_color(0xFF6E00),
        err_color: DEFAULT_ERR_COLOR,
    },
];

pub static DEBUG_THEME: Theme = Theme {
    name: "debug",
    primary_color: hex_to_color(0xb2deff),
    secondary_color: hex_to_color(0xffd68f),
    err_color: DEFAULT_ERR_COLOR,
};
