use ratatui::style::Color;

pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub sel_bg: Color,
    /// Text colour drawn on top of treemap blocks.
    pub dark: Color,
    pub palette: [Color; 8],
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub static THEMES: [Theme; 7] = [
    Theme {
        id: "tokyo-night",
        name: "Tokyo Night",
        bg: rgb(0x1a1b26),
        fg: rgb(0xc0caf5),
        dim: rgb(0x565f89),
        accent: rgb(0x7aa2f7),
        sel_bg: rgb(0x292e42),
        dark: rgb(0x15161e),
        palette: [
            rgb(0x7aa2f7),
            rgb(0xbb9af7),
            rgb(0x9ece6a),
            rgb(0xe0af68),
            rgb(0xf7768e),
            rgb(0x7dcfff),
            rgb(0xff9e64),
            rgb(0x73daca),
        ],
    },
    Theme {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        bg: rgb(0xeff1f5),
        fg: rgb(0x4c4f69),
        dim: rgb(0x9ca0b0),
        accent: rgb(0x1e66f5),
        sel_bg: rgb(0xccd0da),
        dark: rgb(0xdce0e8),
        palette: [
            rgb(0x1e66f5),
            rgb(0x8839ef),
            rgb(0x40a02b),
            rgb(0xdf8e1d),
            rgb(0xd20f39),
            rgb(0x04a5e5),
            rgb(0xfe640b),
            rgb(0x179299),
        ],
    },
    Theme {
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        bg: rgb(0x303446),
        fg: rgb(0xc6d0f5),
        dim: rgb(0x737994),
        accent: rgb(0x8caaee),
        sel_bg: rgb(0x414559),
        dark: rgb(0x232634),
        palette: [
            rgb(0x8caaee),
            rgb(0xca9ee6),
            rgb(0xa6d189),
            rgb(0xe5c890),
            rgb(0xe78284),
            rgb(0x99d1db),
            rgb(0xef9f76),
            rgb(0x81c8be),
        ],
    },
    Theme {
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        bg: rgb(0x24273a),
        fg: rgb(0xcad3f4),
        dim: rgb(0x6e738d),
        accent: rgb(0x8aadf4),
        sel_bg: rgb(0x363a4f),
        dark: rgb(0x181926),
        palette: [
            rgb(0x8aadf4),
            rgb(0xc6a0f6),
            rgb(0xa6da95),
            rgb(0xeed49f),
            rgb(0xed8796),
            rgb(0x91d7e3),
            rgb(0xf5a97f),
            rgb(0x8bd5ca),
        ],
    },
    Theme {
        id: "catppuccin",
        name: "Catppuccin Mocha",
        bg: rgb(0x1e1e2e),
        fg: rgb(0xcdd6f4),
        dim: rgb(0x6c7086),
        accent: rgb(0x89b4fa),
        sel_bg: rgb(0x313244),
        dark: rgb(0x11111b),
        palette: [
            rgb(0x89b4fa),
            rgb(0xcba6f7),
            rgb(0xa6e3a1),
            rgb(0xf9e2af),
            rgb(0xf38ba8),
            rgb(0x89dceb),
            rgb(0xfab387),
            rgb(0x94e2d5),
        ],
    },
    Theme {
        id: "dracula",
        name: "Dracula",
        bg: rgb(0x282a36),
        fg: rgb(0xf8f8f2),
        dim: rgb(0x6272a4),
        accent: rgb(0xbd93f9),
        sel_bg: rgb(0x44475a),
        dark: rgb(0x21222c),
        palette: [
            rgb(0xbd93f9),
            rgb(0xff79c6),
            rgb(0x50fa7b),
            rgb(0xf1fa8c),
            rgb(0xff5555),
            rgb(0x8be9fd),
            rgb(0xffb86c),
            rgb(0x69ff94),
        ],
    },
    Theme {
        id: "one-dark",
        name: "One Dark",
        bg: rgb(0x282c34),
        fg: rgb(0xabb2bf),
        dim: rgb(0x5c6370),
        accent: rgb(0x61afef),
        sel_bg: rgb(0x3e4451),
        dark: rgb(0x21252b),
        palette: [
            rgb(0x61afef),
            rgb(0xc678dd),
            rgb(0x98c379),
            rgb(0xe5c07b),
            rgb(0xe06c75),
            rgb(0x56b6c2),
            rgb(0xd19a66),
            rgb(0xbe5046),
        ],
    },
];

pub fn index_of(id: &str) -> usize {
    THEMES.iter().position(|t| t.id == id).unwrap_or(0)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color::Rgb(m(ar, br), m(ag, bg), m(ab, bb))
        }
        _ => a,
    }
}
