use std::sync::OnceLock;

use num_bigint::BigInt;
use regex::Regex;

use crate::error::{quoted, CodeError, Result};
use crate::number::Number;

pub const MAX_NESTING: usize = 128;
const FORBIDDEN_CHARACTER: char = '§';
const FORBIDDEN_MESSAGE: &str = "We must never speak of §!";
const ORDINALS: [&str; 10] = [
    "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth",
];

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Word,
    Number,
    Text(String),
    Symbol,
    Newline,
    Eof,
}

#[derive(Clone, Debug)]
struct Token<'source> {
    kind: TokenKind,
    text: &'source str,
    line: usize,
}

fn decode_text(text: &str, line: usize) -> Result<String> {
    let value: String = serde_json::from_str(text).map_err(|_| {
        let message = if text.contains(FORBIDDEN_CHARACTER) {
            FORBIDDEN_MESSAGE
        } else {
            "Invalid text literal or escape"
        };
        CodeError::new(line, message)
    })?;
    if !value.contains(FORBIDDEN_CHARACTER) {
        return Ok(value);
    }

    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != FORBIDDEN_CHARACTER {
            output.push(ch);
            continue;
        }
        let mut count = 1;
        while chars.next_if_eq(&FORBIDDEN_CHARACTER).is_some() {
            count += 1;
        }
        if count != 7 {
            return Err(CodeError::new(line, FORBIDDEN_MESSAGE));
        }
        output.push('🐍');
    }
    Ok(output)
}

fn tokenize(source: &str) -> Result<Vec<Token<'_>>> {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let pattern = TOKEN.get_or_init(|| {
        Regex::new(concat!(
            r#"(?P<text>"(?:[^"\\]|\\.)*")"#,
            r"|(?P<number>-?[0-9]+(?:\.[0-9]+)?)",
            r"|(?P<word>[A-Za-z_][A-Za-z_0-9]*)",
            r"|(?P<symbol>[:,?()])|(?P<space>[ \t]+)|(?P<invalid>.)",
        ))
        .expect("valid token pattern")
    });
    let mut tokens = Vec::new();
    // The forbidden section sign takes precedence over other syntax errors.
    let mut first_error = None;
    let mut comment_start = None;
    let mut line_count = 0;
    for (offset, raw) in source.split('\n').enumerate() {
        let line = offset + 1;
        line_count = line;
        let content = raw.trim();
        if comment_start.is_some()
            || content.is_empty()
            || content.starts_with("note:")
            || content.starts_with("comment on one line:")
        {
            if content.contains(FORBIDDEN_CHARACTER) {
                return Err(CodeError::new(line, FORBIDDEN_MESSAGE));
            }
            if content == "end of comment" {
                comment_start = None;
            }
            continue;
        }
        if content == "comment:" {
            comment_start = Some(line);
            continue;
        }
        if content == "end of comment" {
            first_error.get_or_insert_with(|| CodeError::new(line, "Unexpected end of comment"));
            continue;
        }

        for captures in pattern.captures_iter(content) {
            let text = captures.get(0).expect("entire token match").as_str();
            let kind = if captures.name("word").is_some() {
                TokenKind::Word
            } else if captures.name("number").is_some() {
                TokenKind::Number
            } else if captures.name("text").is_some() {
                match decode_text(text, line) {
                    Ok(value) => TokenKind::Text(value),
                    Err(error) if error.message == FORBIDDEN_MESSAGE => return Err(error),
                    Err(error) => {
                        first_error.get_or_insert(error);
                        continue;
                    }
                }
            } else if captures.name("symbol").is_some() {
                TokenKind::Symbol
            } else if captures.name("space").is_some() {
                continue;
            } else {
                if text.starts_with(FORBIDDEN_CHARACTER) {
                    return Err(CodeError::new(line, FORBIDDEN_MESSAGE));
                }
                first_error.get_or_insert_with(|| {
                    let message = if text == "\"" {
                        "Unterminated text".to_owned()
                    } else {
                        format!("Unexpected character {}", quoted(text))
                    };
                    CodeError::new(line, message)
                });
                continue;
            };
            tokens.push(Token { kind, text, line });
        }
        tokens.push(Token {
            kind: TokenKind::Newline,
            text: "",
            line,
        });
    }
    if let Some(line) = comment_start {
        first_error.get_or_insert_with(|| CodeError::new(line, "Missing end of comment"));
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: "",
        line: line_count,
    });
    Ok(tokens)
}

#[derive(Debug)]
pub struct Name {
    pub text: String,
    pub line: usize,
}

impl From<Token<'_>> for Name {
    fn from(token: Token<'_>) -> Self {
        Self {
            text: token.text.to_owned(),
            line: token.line,
        }
    }
}

#[derive(Debug)]
pub struct Expr {
    pub line: usize,
    pub kind: ExprKind,
}

#[derive(Debug)]
pub enum ExprKind {
    Number(Number),
    Text(String),
    Truth(bool),
    Variable(String),
    Group(Box<Expr>),
    Position(Position),
    Named(Named),
    Collection(Vec<Entry>),
}

#[derive(Debug)]
pub struct Position {
    pub index: BigInt,
    pub collection: Box<Expr>,
}

#[derive(Debug)]
pub struct Named {
    pub collection: Box<Expr>,
    pub names: Vec<Name>,
}

#[derive(Debug)]
pub struct Entry {
    pub value: Expr,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum Compare {
    Less,
    Greater,
    Equal,
    NotEqual,
}

const COMPARISONS: [(&[&str], Compare); 4] = [
    (&["less", "than"], Compare::Less),
    (&["greater", "than"], Compare::Greater),
    (&["equal", "to"], Compare::Equal),
    (&["not", "equal", "to"], Compare::NotEqual),
];

#[derive(Debug)]
pub struct Condition {
    pub value: Expr,
    pub comparison: Option<(Compare, Expr)>,
}

#[derive(Debug)]
pub struct Statement {
    pub line: usize,
    pub kind: StatementKind,
}

#[derive(Debug)]
pub enum StatementKind {
    Assign {
        variable: Name,
        value: Expr,
        declare: bool,
    },
    Adjust {
        variable: Name,
        amount: Expr,
        subtract: bool,
    },
    Print {
        value: Expr,
        numeric: bool,
        newline: bool,
    },
    Ask {
        prompt: Expr,
        variable: Name,
    },
    Count(Expr),
    Repeat(Expr, Vec<Statement>),
    While(Condition, Vec<Statement>),
    If {
        condition: Condition,
        body: Vec<Statement>,
        alternative: Vec<Statement>,
    },
    ForEach {
        variable: Name,
        collection: Expr,
        body: Vec<Statement>,
    },
    Put {
        value: Expr,
        target: Expr,
    },
    Remove(Position),
}

pub fn parse(source: &str) -> Result<Vec<Statement>> {
    Parser {
        tokens: tokenize(source)?,
        index: 0,
        nesting: 0,
    }
    .block(None, false)
}

struct Parser<'source> {
    tokens: Vec<Token<'source>>,
    index: usize,
    nesting: usize,
}

impl<'source> Parser<'source> {
    fn peek(&self, ahead: usize) -> &Token<'source> {
        &self.tokens[(self.index + ahead).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token<'source> {
        let token = self.peek(0).clone();
        self.index += 1;
        token
    }

    fn accept(&mut self, words: &[&str]) -> bool {
        if words
            .iter()
            .enumerate()
            .all(|(i, word)| self.peek(i).text == *word)
        {
            self.index += words.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, word: &str) -> Result<()> {
        if self.accept(&[word]) {
            Ok(())
        } else {
            Err(CodeError::new(
                self.peek(0).line,
                format!("Expected {}", quoted(word)),
            ))
        }
    }

    fn end_line(&mut self) -> Result<()> {
        let token = self.peek(0);
        if token.kind != TokenKind::Newline {
            return Err(CodeError::new(
                token.line,
                format!("Unexpected {} at end of line", quoted(token.text)),
            ));
        }
        self.advance();
        Ok(())
    }

    fn identifier(&mut self, description: &str) -> Result<Token<'source>> {
        if self.peek(0).kind != TokenKind::Word {
            return Err(CodeError::new(
                self.peek(0).line,
                format!("Expected {description}"),
            ));
        }
        Ok(self.advance())
    }

    fn variable(&mut self) -> Result<Name> {
        let token = self.identifier("a variable name")?;
        if matches!(token.text, "true" | "false") {
            return Err(CodeError::new(
                token.line,
                format!("'{}' is a truth value", token.text),
            ));
        }
        Ok(token.into())
    }

    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.nesting >= MAX_NESTING {
            return Err(CodeError::new(
                self.peek(0).line,
                "Program is nested too deeply",
            ));
        }
        self.nesting += 1;
        let result = parse(self);
        self.nesting -= 1;
        result
    }

    fn block(
        &mut self,
        opening: Option<&Token<'_>>,
        allow_otherwise: bool,
    ) -> Result<Vec<Statement>> {
        self.nested(|parser| {
            let mut statements = Vec::new();
            while parser.peek(0).kind != TokenKind::Eof {
                let token = parser.peek(0);
                if token.text == "ok" {
                    if opening.is_none() {
                        return Err(CodeError::new(token.line, "Unexpected ok"));
                    }
                    return Ok(statements);
                }
                if token.text == "otherwise" && parser.peek(1).text != "but" {
                    if allow_otherwise {
                        return Ok(statements);
                    }
                    return Err(CodeError::new(token.line, "Unexpected otherwise"));
                }
                statements.push(parser.statement()?);
                parser.end_line()?;
            }
            if let Some(opening) = opening {
                let block_name = if opening.text == "if" { "if" } else { "loop" };
                return Err(CodeError::new(
                    opening.line,
                    format!("Missing ok for {block_name}"),
                ));
            }
            Ok(statements)
        })
    }

    fn closed_block(&mut self, opening: &Token<'_>) -> Result<Vec<Statement>> {
        let body = self.block(Some(opening), false)?;
        self.expect("ok")?;
        Ok(body)
    }

    fn statement(&mut self) -> Result<Statement> {
        use StatementKind::*;

        let start = self.advance();
        let line = start.line;
        let kind = match start.text {
            "remember" => {
                let (variable, value) = if self.peek(0).kind == TokenKind::Word
                    && !matches!(self.peek(0).text, "true" | "false")
                    && self.peek(1).text == "as"
                {
                    let variable = self.variable()?;
                    self.expect("as")?;
                    (variable, self.expression(false)?)
                } else {
                    let value = self.expression(false)?;
                    self.expect("as")?;
                    (self.variable()?, value)
                };
                Assign {
                    variable,
                    value,
                    declare: true,
                }
            }
            "set" => {
                let variable = self.variable()?;
                self.expect("to")?;
                Assign {
                    variable,
                    value: self.expression(false)?,
                    declare: false,
                }
            }
            "add" => {
                let amount = if self.accept(&["one", "to"]) {
                    Expr {
                        line,
                        kind: ExprKind::Number(Number::Whole(1.into())),
                    }
                } else {
                    let amount = self.expression(false)?;
                    self.expect("to")?;
                    amount
                };
                Adjust {
                    variable: self.variable()?,
                    amount,
                    subtract: false,
                }
            }
            "take" => {
                let amount = self.expression(false)?;
                self.expect("from")?;
                Adjust {
                    variable: self.variable()?,
                    amount,
                    subtract: true,
                }
            }
            "say" => {
                let newline = !self.accept(&["on", "one", "line"]);
                Print {
                    value: self.expression(false)?,
                    numeric: false,
                    newline,
                }
            }
            "ask" => {
                let prompt = self.expression(false)?;
                self.expect("and")?;
                self.expect("remember")?;
                self.expect("as")?;
                Ask {
                    prompt,
                    variable: self.variable()?,
                }
            }
            "what" => {
                self.expect("is")?;
                let value = self.expression(false)?;
                self.expect("?")?;
                Print {
                    value,
                    numeric: false,
                    newline: true,
                }
            }
            "how" => {
                self.expect("many")?;
                let kind = if self.accept(&["are", "in"]) {
                    Count(self.expression(false)?)
                } else {
                    let name = self.variable()?;
                    let value = Expr {
                        line: name.line,
                        kind: ExprKind::Variable(name.text),
                    };
                    Print {
                        value,
                        numeric: true,
                        newline: true,
                    }
                };
                self.expect("?")?;
                kind
            }
            "do" => {
                self.expect("this")?;
                let count = self.expression(false)?;
                self.expect("times")?;
                self.header_end()?;
                Repeat(count, self.closed_block(&start)?)
            }
            "while" => {
                let condition = self.condition_header()?;
                While(condition, self.closed_block(&start)?)
            }
            "if" => {
                let condition = self.condition_header()?;
                let body = self.block(Some(&start), true)?;
                let alternative = if self.accept(&["otherwise"]) {
                    self.header_end()?;
                    self.block(Some(&start), false)?
                } else {
                    Vec::new()
                };
                self.expect("ok")?;
                If {
                    condition,
                    body,
                    alternative,
                }
            }
            "for" => {
                self.expect("each")?;
                let variable = self.variable()?;
                self.expect("in")?;
                let collection = self.expression(false)?;
                self.header_end()?;
                ForEach {
                    variable,
                    collection,
                    body: self.closed_block(&start)?,
                }
            }
            "put" => {
                let value = self.expression(false)?;
                self.expect("into")?;
                Put {
                    value,
                    target: self.expression(false)?,
                }
            }
            "remove" => match self.expression(false)?.kind {
                ExprKind::Position(position) => Remove(position),
                _ => return Err(CodeError::new(line, "Remove requires a position")),
            },
            _ if start.kind == TokenKind::Word && self.accept(&["but"]) => {
                if matches!(start.text, "true" | "false") {
                    return Err(CodeError::new(line, "Expected a variable before 'but'"));
                }
                let amount = self.expression(false)?;
                let subtract = if self.accept(&["more"]) {
                    false
                } else if self.accept(&["less"]) {
                    true
                } else {
                    return Err(CodeError::new(
                        self.peek(0).line,
                        "Expected 'more' or 'less'",
                    ));
                };
                Adjust {
                    variable: start.into(),
                    amount,
                    subtract,
                }
            }
            _ => {
                return Err(CodeError::new(
                    line,
                    format!("Unknown instruction {}", quoted(start.text)),
                ))
            }
        };
        Ok(Statement { line, kind })
    }

    fn header_end(&mut self) -> Result<()> {
        self.expect(":")?;
        self.end_line()
    }

    fn condition_header(&mut self) -> Result<Condition> {
        let value = self.expression(false)?;
        let comparison = if self.accept(&["is"]) {
            let operator = COMPARISONS
                .iter()
                .find_map(|(words, operator)| self.accept(words).then_some(*operator));
            let line = self.peek(0).line;
            let right = self.expression(false)?;
            if operator.is_none() && self.peek(0).text != ":" {
                return Err(CodeError::new(line, "Expected a comparison after 'is'"));
            }
            Some((operator.unwrap_or(Compare::Equal), right))
        } else {
            None
        };
        self.header_end()?;
        Ok(Condition { value, comparison })
    }

    fn expression(&mut self, entry: bool) -> Result<Expr> {
        self.nested(|parser| parser.expression_inner(entry))
    }

    fn expression_inner(&mut self, entry: bool) -> Result<Expr> {
        let token = self.advance();
        let line = token.line;
        let kind = match token.text {
            "all" if self.accept(&["of", "these", ":"]) => {
                ExprKind::Collection(self.collection(line, entry)?)
            }
            "the" if self.peek(1).text == "of" => {
                let ordinal = self.advance();
                let index = ORDINALS
                    .iter()
                    .position(|word| *word == ordinal.text)
                    .ok_or_else(|| {
                        CodeError::new(
                            ordinal.line,
                            format!("Unknown ordinal {}", quoted(ordinal.text)),
                        )
                    })?
                    + 1;
                self.expect("of")?;
                ExprKind::Position(Position {
                    index: index.into(),
                    collection: Box::new(self.expression(entry)?),
                })
            }
            "position" if self.peek(0).kind == TokenKind::Number => {
                let number = self.advance();
                let Number::Whole(index) = Self::number(&number)? else {
                    return Err(CodeError::new(
                        number.line,
                        "Expected a whole number for the position",
                    ));
                };
                self.expect("of")?;
                ExprKind::Position(Position {
                    index,
                    collection: Box::new(self.expression(entry)?),
                })
            }
            _ => {
                let mut value = self.primary(token)?;
                if !entry && self.accept(&["named"]) {
                    let mut names = vec![self.identifier("an item name")?.into()];
                    while self.accept(&["named"]) {
                        names.push(self.identifier("an item name")?.into());
                    }
                    value = Expr {
                        line: names.last().map_or(line, |name: &Name| name.line),
                        kind: ExprKind::Named(Named {
                            collection: Box::new(value),
                            names,
                        }),
                    };
                }
                return Ok(value);
            }
        };
        Ok(Expr { line, kind })
    }

    fn primary(&mut self, token: Token<'_>) -> Result<Expr> {
        let kind = match token.kind {
            TokenKind::Number => ExprKind::Number(Self::number(&token)?),
            TokenKind::Text(value) => ExprKind::Text(value),
            TokenKind::Word if token.text == "true" => ExprKind::Truth(true),
            TokenKind::Word if token.text == "false" => ExprKind::Truth(false),
            TokenKind::Word if token.text == "arrow" && self.accept(&["("]) => {
                let direction = self.advance();
                let value = match direction.text {
                    "up" => "⬆️",
                    "down" => "⬇️",
                    "left" => "⬅️",
                    "right" => "➡️",
                    _ => {
                        return Err(CodeError::new(
                            direction.line,
                            "Expected arrow direction up, down, left, or right",
                        ))
                    }
                };
                self.expect(")")?;
                ExprKind::Text(value.to_owned())
            }
            TokenKind::Word => ExprKind::Variable(token.text.to_owned()),
            TokenKind::Symbol if token.text == "(" => {
                let value = self.expression(false)?;
                self.expect(")")?;
                ExprKind::Group(Box::new(value))
            }
            _ => return Err(CodeError::new(token.line, "Expected a value")),
        };
        Ok(Expr {
            line: token.line,
            kind,
        })
    }

    fn number(token: &Token<'_>) -> Result<Number> {
        Number::parse(token.text)
            .ok_or_else(|| CodeError::new(token.line, "Number literal is too large"))
    }

    fn collection(&mut self, line: usize, entry: bool) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        if self.peek(0).kind == TokenKind::Newline {
            self.end_line()?;
            while !self.accept(&["ok"]) {
                if self.peek(0).kind == TokenKind::Eof {
                    return Err(CodeError::new(line, "Missing ok for collection"));
                }
                entries.push(self.entry()?);
                self.accept(&[","]);
                self.end_line()?;
            }
        } else if self.peek(0).text != ")" {
            if entry {
                return Err(CodeError::new(
                    line,
                    "Wrap an inline nested collection in parentheses",
                ));
            }
            entries.push(self.entry()?);
            while self.accept(&[","]) {
                if self.peek(0).kind == TokenKind::Newline || self.peek(0).text == ")" {
                    break;
                }
                entries.push(self.entry()?);
            }
        }
        Ok(entries)
    }

    fn entry(&mut self) -> Result<Entry> {
        let value = self.expression(true)?;
        let name = if self.accept(&["named"]) {
            Some(self.identifier("an item name")?.text.to_owned())
        } else {
            None
        };
        Ok(Entry { value, name })
    }
}
