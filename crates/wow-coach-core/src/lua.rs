//! A parser for the restricted Lua subset the collector writes.
//!
//! This is deliberately **not** a Lua interpreter. It reads data, and anything
//! that is not data is an error rather than something to evaluate. The addon
//! and this parser are written together, so the subset is narrow on purpose:
//! assignments of table constructors holding strings, numbers, booleans and
//! nested tables, and nothing else.
//!
//! A file outside the subset means the collector has a bug. Rejecting it loudly
//! is better than recovering something plausible from it.

use std::collections::BTreeMap;
use std::fmt;

/// A value in the restricted subset.
#[derive(Debug, Clone, PartialEq)]
pub enum LuaValue {
    String(String),
    Number(f64),
    Bool(bool),
    Table(LuaTable),
}

/// A Lua table. Keys are kept separate by kind because the collector emits both
/// `["name"]` and `[1]`, and conflating them loses the distinction between a
/// list and a record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LuaTable {
    pub map: BTreeMap<String, LuaValue>,
    pub array: Vec<LuaValue>,
}

impl LuaTable {
    pub fn get(&self, key: &str) -> Option<&LuaValue> {
        self.map.get(key)
    }
}

impl LuaValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// Numbers arrive as f64 because that is what Lua has. Anything with a
    /// fractional part is not an integer, and saying so beats truncating.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) if value.fract() == 0.0 && value.is_finite() => Some(*value as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&LuaTable> {
        match self {
            Self::Table(table) => Some(table),
            _ => None,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Number(_) => "number",
            Self::Bool(_) => "boolean",
            Self::Table(_) => "table",
        }
    }
}

/// A parse failure, carrying the byte offset and the line so a bad file can
/// actually be looked at rather than guessed about.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
    pub line: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse a SavedVariables file into its top-level assignments.
///
/// WoW writes one `GlobalName = { ... }` per saved variable, so the result is
/// keyed by global name.
pub fn parse_saved_variables(input: &str) -> Result<BTreeMap<String, LuaValue>, ParseError> {
    let mut parser = Parser::new(input);
    let mut globals = BTreeMap::new();

    parser.skip_trivia();
    while !parser.at_end() {
        let name = parser.parse_identifier()?;
        parser.skip_trivia();
        parser.expect(b'=')?;
        parser.skip_trivia();
        let value = parser.parse_value()?;
        globals.insert(name, value);

        parser.skip_trivia();
        // WoW does not write a separator between globals, but tolerating a
        // stray semicolon costs nothing and avoids a pointless failure.
        if parser.peek() == Some(b';') {
            parser.bump();
            parser.skip_trivia();
        }
    }

    Ok(globals)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

/// Deep nesting is a sign of a malformed or hostile file rather than anything
/// the collector produces; the real schema is about five levels deep.
const MAX_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            pos: 0,
            depth: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.pos += 1;
        }
        byte
    }

    fn line(&self) -> usize {
        self.bytes[..self.pos.min(self.bytes.len())]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count()
            + 1
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            message: message.into(),
            offset: self.pos,
            line: self.line(),
        })
    }

    /// Whitespace and comments. The collector never writes a comment, but a
    /// human looking at a fixture may add one, and rejecting that helps nobody.
    fn skip_trivia(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.pos += 1;
            }
            if self.bytes[self.pos..].starts_with(b"--") {
                self.pos += 2;
                if self.bytes[self.pos..].starts_with(b"[[") {
                    self.pos += 2;
                    while !self.at_end() && !self.bytes[self.pos..].starts_with(b"]]") {
                        self.pos += 1;
                    }
                    self.pos = (self.pos + 2).min(self.bytes.len());
                } else {
                    while !self.at_end() && self.peek() != Some(b'\n') {
                        self.pos += 1;
                    }
                }
                continue;
            }
            return;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            return Ok(());
        }
        let found = match self.peek() {
            Some(found) => format!("{:?}", found as char),
            None => "end of input".to_string(),
        };
        self.error(format!("expected {:?}, found {}", byte as char, found))
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_') {
            self.pos += 1;
        }
        if start == self.pos {
            return self.error("expected a global name");
        }
        Ok(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
    }

    fn parse_value(&mut self) -> Result<LuaValue, ParseError> {
        match self.peek() {
            Some(b'{') => self.parse_table(),
            Some(b'"') => Ok(LuaValue::String(self.parse_string()?)),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.parse_number(),
            Some(b't') if self.bytes[self.pos..].starts_with(b"true") => {
                self.pos += 4;
                Ok(LuaValue::Bool(true))
            }
            Some(b'f') if self.bytes[self.pos..].starts_with(b"false") => {
                self.pos += 5;
                Ok(LuaValue::Bool(false))
            }
            Some(b'n') if self.bytes[self.pos..].starts_with(b"nil") => {
                // The collector never stores nil; a key it cannot fill is
                // simply absent. A literal nil means something else wrote here.
                self.error("nil is not part of the collector's subset")
            }
            Some(byte) => self.error(format!(
                "unexpected {:?}: the subset holds only tables, strings, numbers and booleans",
                byte as char
            )),
            None => self.error("unexpected end of input"),
        }
    }

    fn parse_table(&mut self) -> Result<LuaValue, ParseError> {
        if self.depth >= MAX_DEPTH {
            return self.error(format!("tables nested deeper than {MAX_DEPTH}"));
        }
        self.expect(b'{')?;
        self.depth += 1;

        let mut table = LuaTable::default();
        // Lua's own implicit array index, for a file written without explicit
        // `[n] =` keys. WoW writes them explicitly, but hand-written fixtures
        // often do not.
        let mut implicit_index: i64 = 1;

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    self.depth -= 1;
                    return Ok(LuaValue::Table(table));
                }
                None => return self.error("unterminated table"),
                _ => {}
            }

            if self.peek() == Some(b'[') {
                self.pos += 1;
                self.skip_trivia();
                let key = match self.peek() {
                    Some(b'"') => TableKey::Name(self.parse_string()?),
                    Some(byte) if byte == b'-' || byte.is_ascii_digit() => {
                        match self.parse_number()? {
                            LuaValue::Number(value) if value.fract() == 0.0 => {
                                TableKey::Index(value as i64)
                            }
                            _ => return self.error("table keys must be strings or integers"),
                        }
                    }
                    _ => return self.error("table keys must be strings or integers"),
                };
                self.skip_trivia();
                self.expect(b']')?;
                self.skip_trivia();
                self.expect(b'=')?;
                self.skip_trivia();
                let value = self.parse_value()?;
                self.insert(&mut table, key, value)?;
            } else {
                let value = self.parse_value()?;
                self.insert(&mut table, TableKey::Index(implicit_index), value)?;
                implicit_index += 1;
            }

            self.skip_trivia();
            match self.peek() {
                Some(b',') | Some(b';') => {
                    self.pos += 1;
                }
                Some(b'}') => {}
                _ => return self.error("expected ',' or '}' after a table entry"),
            }
        }
    }

    /// Array entries must arrive in order and without gaps. WoW's writer always
    /// emits them that way; anything else means the file was produced by
    /// something else, and silently reordering it would hide that.
    fn insert(
        &self,
        table: &mut LuaTable,
        key: TableKey,
        value: LuaValue,
    ) -> Result<(), ParseError> {
        match key {
            TableKey::Name(name) => {
                table.map.insert(name, value);
            }
            TableKey::Index(index) => {
                let expected = table.array.len() as i64 + 1;
                if index != expected {
                    return self.error(format!(
                        "array index {index} out of order (expected {expected})"
                    ));
                }
                table.array.push(value);
            }
        }
        Ok(())
    }

    fn parse_number(&mut self) -> Result<LuaValue, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.pos += 1;
            }
        }

        let text = String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned();
        match text.parse::<f64>() {
            Ok(value) => Ok(LuaValue::Number(value)),
            Err(_) => self.error(format!("{text:?} is not a number")),
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = match self.bump() {
                Some(byte) => byte,
                None => return self.error("unterminated string"),
            };
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escape = match self.bump() {
                        Some(escape) => escape,
                        None => return self.error("unterminated escape"),
                    };
                    match escape {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'a' => out.push('\u{7}'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'v' => out.push('\u{b}'),
                        b'\\' => out.push('\\'),
                        b'"' => out.push('"'),
                        b'\'' => out.push('\''),
                        // Lua's %q writes a backslash followed by a real
                        // newline for an embedded newline.
                        b'\n' => out.push('\n'),
                        digit if digit.is_ascii_digit() => {
                            // A decimal byte escape of up to three digits.
                            let mut value = u32::from(digit - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(next) if next.is_ascii_digit() => {
                                        value = value * 10 + u32::from(next - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            if value > 255 {
                                return self.error(format!("byte escape \\{value} out of range"));
                            }
                            // Bytes are collected raw and decoded with the rest
                            // of the string below.
                            out.push(value as u8 as char);
                        }
                        other => {
                            return self.error(format!("unsupported escape \\{}", other as char))
                        }
                    }
                }
                _ => {
                    // Collect the raw byte; multi-byte UTF-8 sequences arrive
                    // one byte at a time and are reassembled here.
                    let start = self.pos - 1;
                    let len = utf8_len(byte);
                    if len > 1 {
                        self.pos = (start + len).min(self.bytes.len());
                    }
                    match std::str::from_utf8(&self.bytes[start..self.pos]) {
                        Ok(text) => out.push_str(text),
                        // WoW writes UTF-8, but an addon can store arbitrary
                        // bytes. Replacing them keeps the rest of the file
                        // readable rather than failing the whole parse.
                        Err(_) => out.push(char::REPLACEMENT_CHARACTER),
                    }
                }
            }
        }
    }
}

fn utf8_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

enum TableKey {
    Name(String),
    Index(i64),
}

/// Convert to `serde_json::Value` so the typed models can be deserialized with
/// serde rather than by hand.
///
/// A table with both named and array entries would be ambiguous as JSON; the
/// collector never writes one, so it is an error rather than a guess.
pub fn to_json(value: &LuaValue) -> Result<serde_json::Value, String> {
    Ok(match value {
        LuaValue::String(text) => serde_json::Value::String(text.clone()),
        LuaValue::Bool(flag) => serde_json::Value::Bool(*flag),
        // Lua has one number type, so every count, level and timestamp arrives
        // as an f64. Emitting those as JSON floats would make every integer
        // field in the schema fail to deserialize, so a value with no
        // fractional part is narrowed back to an integer here.
        LuaValue::Number(number) => {
            if number.fract() == 0.0
                && number.is_finite()
                && *number >= i64::MIN as f64
                && *number <= i64::MAX as f64
            {
                serde_json::Value::Number(serde_json::Number::from(*number as i64))
            } else {
                serde_json::Number::from_f64(*number)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| format!("{number} cannot be represented as JSON"))?
            }
        }
        LuaValue::Table(table) => {
            if !table.map.is_empty() && !table.array.is_empty() {
                return Err("table mixes named keys and array entries".to_string());
            }
            if table.map.is_empty() && !table.array.is_empty() {
                let mut items = Vec::with_capacity(table.array.len());
                for item in &table.array {
                    items.push(to_json(item)?);
                }
                serde_json::Value::Array(items)
            } else {
                let mut object = serde_json::Map::new();
                for (key, item) in &table.map {
                    object.insert(key.clone(), to_json(item)?);
                }
                serde_json::Value::Object(object)
            }
        }
    })
}

/// An empty table is ambiguous: it could be an empty list or an empty record.
/// `to_json` calls it a record, so a field typed as a list needs this.
pub fn to_json_as_array(value: &LuaValue) -> Result<serde_json::Value, String> {
    match value {
        LuaValue::Table(table) if table.map.is_empty() => {
            let mut items = Vec::with_capacity(table.array.len());
            for item in &table.array {
                items.push(to_json(item)?);
            }
            Ok(serde_json::Value::Array(items))
        }
        LuaValue::Table(_) => Err("expected a list, found a table with named keys".to_string()),
        other => Err(format!("expected a list, found {}", other.kind())),
    }
}
