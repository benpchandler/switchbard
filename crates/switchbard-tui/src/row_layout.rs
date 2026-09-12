//! One bounded task-row presentation contract, shared by view persistence and layout.
use mlua::{Table, Value};
use ratatui::widgets::{Paragraph, Wrap};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TitleWrapping {
    #[default]
    Off,
    Two,
    Three,
    Six,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RowLayout {
    pub wrapping: TitleWrapping,
    pub spaced: bool,
}

impl RowLayout {
    pub fn lines(self) -> u16 {
        match self.wrapping {
            TitleWrapping::Off => 1,
            TitleWrapping::Two => 2,
            TitleWrapping::Three => 3,
            TitleWrapping::Six => 6,
        }
    }

    pub fn cycle_wrapping(&mut self) {
        self.wrapping = match self.wrapping {
            TitleWrapping::Off => TitleWrapping::Two,
            TitleWrapping::Two => TitleWrapping::Three,
            TitleWrapping::Three => TitleWrapping::Six,
            TitleWrapping::Six => TitleWrapping::Off,
        };
    }

    pub fn wrap_label(self) -> &'static str {
        match self.wrapping {
            TitleWrapping::Off => "off (1 line)",
            TitleWrapping::Two => "up to 2 lines",
            TitleWrapping::Three => "up to 3 lines",
            TitleWrapping::Six => "up to 6 lines",
        }
    }

    pub fn spacing_label(self) -> &'static str {
        if self.spaced {
            "blank line"
        } else {
            "compact"
        }
    }

    pub fn label(self) -> Option<String> {
        (self != Self::default()).then(|| {
            format!(
                "rows:{}{}",
                self.lines(),
                if self.spaced { "+space" } else { "" }
            )
        })
    }

    pub(crate) fn from_lua(entry: &Table) -> Result<Self, String> {
        let lines = integer_setting(entry, "title_lines", 1)?;
        let wrapping = match lines {
            1 => TitleWrapping::Off,
            2 => TitleWrapping::Two,
            3 => TitleWrapping::Three,
            6 => TitleWrapping::Six,
            _ => return Err("unsupported title_lines; expected 1, 2, 3 or 6".into()),
        };
        let spacing = integer_setting(entry, "row_spacing", 0)?;
        if spacing > 1 {
            return Err("unsupported row_spacing; expected 0 or 1".into());
        }
        Ok(Self {
            wrapping,
            spaced: spacing == 1,
        })
    }

    pub(crate) fn to_lua(self) -> String {
        if self == Self::default() {
            return String::new();
        }
        format!(
            ", title_lines = {}, row_spacing = {}",
            self.lines(),
            u8::from(self.spaced)
        )
    }

    pub(crate) fn title_height(self, title: &str, width: u16) -> u16 {
        if self.lines() == 1 || width == 0 {
            return 1;
        }
        paragraph(title)
            .line_count(width)
            .clamp(1, self.lines() as usize) as u16
    }
}

// mlua's integer conversion accepts fractional values and numeric strings. A
// persisted layout must express an exact supported value before it can be saved.
fn integer_setting(entry: &Table, key: &str, default: u16) -> Result<u16, String> {
    let invalid = || format!("unsupported {key}; expected an exact nonnegative integer");
    match entry.get::<Value>(key).map_err(|e| e.to_string())? {
        Value::Nil => Ok(default),
        Value::Integer(value) => u16::try_from(value).map_err(|_| invalid()),
        Value::Number(value)
            if value.is_finite()
                && value.fract() == 0.0
                && (0.0..=u16::MAX as f64).contains(&value) =>
        {
            Ok(value as u16)
        }
        _ => Err(invalid()),
    }
}

// At most 4096 Unicode scalar values enter the wrapper. Controls cannot move the
// cursor; source text and the full detail view remain unchanged.
pub(crate) fn title_text(title: &str) -> String {
    title
        .chars()
        .take(4096)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub(crate) fn paragraph(title: &str) -> Paragraph<'_> {
    Paragraph::new(title).wrap(Wrap { trim: false })
}
