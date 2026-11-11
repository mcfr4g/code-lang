use std::fmt;

pub type Result<T> = std::result::Result<T, CodeError>;

#[derive(Debug)]
pub struct CodeError {
    pub line: usize,
    pub message: String,
    awaiting_input: bool,
}

impl CodeError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            awaiting_input: false,
        }
    }

    pub(crate) fn no_input(line: usize) -> Self {
        Self {
            awaiting_input: true,
            ..Self::new(line, "No input available")
        }
    }

    pub fn is_awaiting_input(&self) -> bool {
        self.awaiting_input
    }
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Line {}: {}", self.line, self.message)
    }
}

pub fn quoted(text: &str) -> String {
    let delimiter = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(delimiter);
    for ch in text.chars() {
        match ch {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\\' => result.push_str("\\\\"),
            ch if ch == delimiter => {
                result.push('\\');
                result.push(ch);
            }
            ch if ch.is_ascii_control() => {
                use fmt::Write as _;
                write!(result, "\\x{:02x}", u32::from(ch)).unwrap();
            }
            ch => result.push(ch),
        }
    }
    result.push(delimiter);
    result
}
