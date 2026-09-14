use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
    pub selection: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: Color::Rgb(105, 214, 208),
            secondary: Color::Rgb(194, 169, 255),
            accent: Color::Rgb(125, 169, 255),
            background: Color::Rgb(9, 16, 25),
            surface: Color::Rgb(19, 29, 41),
            text: Color::Rgb(232, 240, 247),
            muted: Color::Rgb(130, 145, 165),
            selection: Color::Rgb(34, 54, 80),
        }
    }
}
