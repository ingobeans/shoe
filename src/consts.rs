use crossterm::style::Color;

pub static PRIMARY_COLOR: Color = Color::Rgb {
    r: 255,
    g: 193,
    b: 69,
};

pub static SECONDARY_COLOR: Color = Color::Rgb {
    r: 91,
    g: 95,
    b: 151,
};

pub static ERR_COLOR: Color = Color::Red;

pub static HELP_MESSAGE: &'static str = "
--help (-h)   - displays this help message
--no-history  - dont store history in ~/.shoehistory
--no-rc       - dont run startup commands from ~/.shoerc";
