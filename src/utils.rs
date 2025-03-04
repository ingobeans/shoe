use crossterm::style::Color;

pub static DEFAULT_ERR_COLOR: Color = Color::Red;

pub static HELP_MESSAGE: &'static str = "
--help (-h)   - displays this help message
--no-history  - dont store history in ~/.shoehistory
--no-rc       - dont run startup commands from ~/.shoerc";

pub struct Theme<'a> {
    pub name: &'a str,
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
