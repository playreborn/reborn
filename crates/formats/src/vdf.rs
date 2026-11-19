//! Text KeyValues ("VDF"), the format of `libraryfolders.vdf`,
//! `appmanifest_*.acf` and many other files Steam keeps on disk.
//!
//! # Format
//!
//! ```text
//! "AppState"                      // a key followed by an object
//! {
//!     "appid"     "228980"        // a key followed by a string value
//!     "UserConfig"
//!     {
//!         "language"  "english"
//!     }
//! }
//! ```
//!
//! - Keys and values are strings. There is no number or boolean type.
//! - Strings are usually quoted. Unquoted tokens end at whitespace, `"`, `{` or `}`.
//! - Inside quoted strings, `\\`, `\"`, `\n` and `\t` are escapes. Any other
//!   backslash is kept literally, so hand-written paths like `"C:\Games"` survive.
//! - `//` starts a comment that runs to the end of the line.
//! - Keys may repeat; order and duplicates are preserved.
//! - A leading UTF-8 byte order mark is ignored.
//!
//! `#include`/`#base` directives and conditionals such as `[$WIN32]` appear
//! in Valve's UI resource files but not in Steam's data files. They are
//! rejected with [`ParseErrorKind::Unsupported`] instead of being silently
//! misinterpreted.
//!
//! [`write`](fn@write) produces the layout Steam itself uses: tab indentation, two
//! tabs between a key and its string value, and only `\` and `"` escaped
//! (newlines inside values are written literally). Checked against real
//! Steam files: `appmanifest_*.acf`, `libraryfolders.vdf`, `loginusers.vdf`,
//! `config.vdf` and `localconfig.vdf` are reproduced byte for byte.

use std::fmt;

/// Maximum object nesting depth accepted by [`parse`].
///
/// Steam's files nest only a few levels deep; the limit protects against
/// memory exhaustion from malicious input.
pub const MAX_DEPTH: usize = 128;

/// A value: either a string or a nested object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A string. Numbers are stored as strings, too.
    String(String),
    /// A nested object.
    Object(Object),
}

impl Value {
    /// Returns the string if this is a string value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text.as_str()),
            Value::Object(_) => None,
        }
    }

    /// Returns the object if this is an object value.
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Value::String(_) => None,
            Value::Object(object) => Some(object),
        }
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Value::String(text)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Value::String(text.to_owned())
    }
}

impl From<Object> for Value {
    fn from(object: Object) -> Self {
        Value::Object(object)
    }
}

/// An ordered list of key/value pairs.
///
/// Order and duplicate keys are preserved exactly as they appear in the
/// input. Lookups by key are ASCII case-insensitive, like Valve's own
/// KeyValues implementation, because Steam's files are not consistent about
/// key casing (for example `"LibraryFolders"` in older and `"libraryfolders"`
/// in newer files).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Object {
    entries: Vec<(String, Value)>,
}

impl Object {
    /// Creates an empty object.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of entries, counting duplicates.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the object has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all entries in their original order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    /// Returns the value of the first entry whose key matches
    /// (ASCII case-insensitive).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.position(key).map(|index| &self.entries[index].1)
    }

    /// Returns the values of all entries whose key matches
    /// (ASCII case-insensitive), in order.
    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
        self.entries
            .iter()
            .filter(move |(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
    }

    /// Returns the first matching value if it is a string.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// Returns the first matching value if it is an object.
    pub fn get_object(&self, key: &str) -> Option<&Object> {
        self.get(key).and_then(Value::as_object)
    }

    /// Mutable access to the first matching value if it is an object.
    pub fn get_object_mut(&mut self, key: &str) -> Option<&mut Object> {
        let index = self.position(key)?;
        match &mut self.entries[index].1 {
            Value::Object(object) => Some(object),
            Value::String(_) => None,
        }
    }

    /// Appends an entry, even if the key already exists.
    pub fn push(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.entries.push((key.into(), value.into()));
    }

    /// Replaces the value of the first matching entry (keeping its original
    /// key spelling and position) or appends a new entry.
    ///
    /// Returns the previous value, if any.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        let key = key.into();
        let value = value.into();
        match self.position(&key) {
            Some(index) => Some(std::mem::replace(&mut self.entries[index].1, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    /// Removes the first matching entry and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.position(key).map(|index| self.entries.remove(index).1)
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
    }
}

/// Formats the object as KeyValues text, see [`write`](fn@write).
impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&write(self))
    }
}

/// Error returned by [`parse`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at line {line}, column {column}")]
pub struct ParseError {
    /// 1-based line of the offending token.
    pub line: usize,
    /// 1-based column of the offending token, counted in characters.
    pub column: usize,
    /// What went wrong.
    pub kind: ParseErrorKind,
}

/// The kind of a [`ParseError`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// A quoted string is not closed before the end of the input.
    #[error("unterminated quoted string")]
    UnterminatedString,
    /// A `}` appears without a matching `{`.
    #[error("unexpected `}}` without a matching `{{`")]
    UnexpectedClosingBrace,
    /// A `{` appears where a key was expected.
    #[error("expected a key, found `{{`")]
    ExpectedKey,
    /// A key is not followed by a string value or `{`.
    #[error("expected a value or `{{` after the key")]
    ExpectedValue,
    /// The input ends inside an object.
    #[error("unexpected end of input, missing `}}`")]
    UnclosedObject,
    /// Objects are nested deeper than [`MAX_DEPTH`].
    #[error("objects are nested deeper than {} levels", MAX_DEPTH)]
    TooDeep,
    /// A KeyValues feature that this parser deliberately does not support.
    #[error("unsupported KeyValues feature: {0}")]
    Unsupported(&'static str),
}

/// Parses KeyValues text into an [`Object`] holding all top-level entries.
///
/// A typical Steam file has exactly one top-level entry (e.g. `"AppState"`),
/// whose value is an object.
pub fn parse(input: &str) -> Result<Object, ParseError> {
    let mut lexer = Lexer::new(input);
    // Objects that are still open, together with the key they belong to.
    let mut open: Vec<(String, Object)> = Vec::new();
    let mut current = Object::new();

    loop {
        let Some(token) = lexer.next_token()? else {
            if open.is_empty() {
                return Ok(current);
            }
            return Err(lexer.error_here(ParseErrorKind::UnclosedObject));
        };

        let key = match token.kind {
            TokenKind::Text { text, quoted } => {
                if !quoted && is_directive(&text) {
                    return Err(token
                        .at
                        .error(ParseErrorKind::Unsupported("`#include`/`#base` directives")));
                }
                text
            }
            TokenKind::Close => {
                let Some((key, mut parent)) = open.pop() else {
                    return Err(token.at.error(ParseErrorKind::UnexpectedClosingBrace));
                };
                parent.push(key, std::mem::take(&mut current));
                current = parent;
                continue;
            }
            TokenKind::Open => return Err(token.at.error(ParseErrorKind::ExpectedKey)),
            TokenKind::Conditional => return Err(token.at.error(UNSUPPORTED_CONDITIONAL)),
        };

        let Some(value) = lexer.next_token()? else {
            return Err(lexer.error_here(ParseErrorKind::ExpectedValue));
        };
        match value.kind {
            TokenKind::Text { text, .. } => current.push(key, text),
            TokenKind::Open => {
                if open.len() >= MAX_DEPTH {
                    return Err(value.at.error(ParseErrorKind::TooDeep));
                }
                open.push((key, std::mem::take(&mut current)));
            }
            TokenKind::Close => return Err(value.at.error(ParseErrorKind::ExpectedValue)),
            TokenKind::Conditional => return Err(value.at.error(UNSUPPORTED_CONDITIONAL)),
        }
    }
}

/// Serializes an object as KeyValues text in the layout Steam uses.
///
/// `parse(&write(&object))` always returns an object equal to `object`.
pub fn write(object: &Object) -> String {
    let mut out = String::new();
    write_object(&mut out, object, 0);
    out
}

const UNSUPPORTED_CONDITIONAL: ParseErrorKind =
    ParseErrorKind::Unsupported("conditionals such as `[$WIN32]`");

fn is_directive(text: &str) -> bool {
    text.eq_ignore_ascii_case("#include") || text.eq_ignore_ascii_case("#base")
}

fn write_object(out: &mut String, object: &Object, depth: usize) {
    for (key, value) in object.iter() {
        indent(out, depth);
        write_quoted(out, key);
        match value {
            Value::String(text) => {
                out.push_str("\t\t");
                write_quoted(out, text);
                out.push('\n');
            }
            Value::Object(child) => {
                out.push('\n');
                indent(out, depth);
                out.push_str("{\n");
                write_object(out, child, depth + 1);
                indent(out, depth);
                out.push_str("}\n");
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    out.extend(std::iter::repeat_n('\t', depth));
}

/// Writes a quoted string. Only `\` and `"` are escaped; newlines and tabs
/// are written literally, as Steam does (multi-line values in `config.vdf`).
fn write_quoted(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out.push('"');
}

#[derive(Debug)]
enum TokenKind {
    Text { text: String, quoted: bool },
    Open,
    Close,
    Conditional,
}

#[derive(Debug)]
struct Token {
    kind: TokenKind,
    /// Where the token starts.
    at: Position,
}

#[derive(Clone, Copy, Debug)]
struct Position {
    line: usize,
    column: usize,
}

impl Position {
    fn error(self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            line: self.line,
            column: self.column,
            kind,
        }
    }
}

struct Lexer<'a> {
    chars: std::str::Chars<'a>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        Self {
            chars: input.chars(),
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.clone().next()
    }

    fn peek_second(&self) -> Option<char> {
        let mut ahead = self.chars.clone();
        ahead.next();
        ahead.next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn error_here(&self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            line: self.line,
            column: self.column,
            kind,
        }
    }

    /// Skips whitespace and `//` comments.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_second() == Some('/') => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        self.skip_trivia();
        let (line, column) = (self.line, self.column);
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        let kind = match c {
            '{' => {
                self.bump();
                TokenKind::Open
            }
            '}' => {
                self.bump();
                TokenKind::Close
            }
            '[' => {
                self.bump();
                TokenKind::Conditional
            }
            '"' => {
                self.bump();
                let text = self.quoted().ok_or(ParseError {
                    line,
                    column,
                    kind: ParseErrorKind::UnterminatedString,
                })?;
                TokenKind::Text { text, quoted: true }
            }
            _ => TokenKind::Text {
                text: self.unquoted(),
                quoted: false,
            },
        };
        Ok(Some(Token {
            kind,
            at: Position { line, column },
        }))
    }

    /// Reads the rest of a quoted string after the opening quote.
    /// Returns `None` if the input ends before the closing quote.
    fn quoted(&mut self) -> Option<String> {
        let mut text = String::new();
        loop {
            match self.bump()? {
                '"' => return Some(text),
                '\\' => {
                    let escaped = match self.peek() {
                        Some('\\') => Some('\\'),
                        Some('"') => Some('"'),
                        Some('n') => Some('\n'),
                        Some('t') => Some('\t'),
                        _ => None,
                    };
                    match escaped {
                        Some(c) => {
                            self.bump();
                            text.push(c);
                        }
                        // Unknown escape: keep the backslash literally.
                        None => text.push('\\'),
                    }
                }
                c => text.push(c),
            }
        }
    }

    fn unquoted(&mut self) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || matches!(c, '"' | '{' | '}') {
                break;
            }
            text.push(c);
            self.bump();
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_of(input: &str) -> ParseError {
        parse(input).expect_err("input should be rejected")
    }

    #[test]
    fn parses_nested_objects() {
        let doc = parse("\"AppState\"\n{\n\t\"appid\"\t\t\"228980\"\n\t\"UserConfig\"\n\t{\n\t\t\"language\"\t\t\"english\"\n\t}\n}\n").unwrap();
        let state = doc.get_object("AppState").unwrap();
        assert_eq!(state.get_str("appid"), Some("228980"));
        let config = state.get_object("UserConfig").unwrap();
        assert_eq!(config.get_str("language"), Some("english"));
    }

    #[test]
    fn empty_input_is_an_empty_object() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("  \n// only a comment\n").unwrap().is_empty());
    }

    #[test]
    fn parses_empty_objects() {
        let doc = parse("\"a\" {}").unwrap();
        assert!(doc.get_object("a").unwrap().is_empty());
    }

    #[test]
    fn preserves_order_and_duplicate_keys() {
        let doc = parse("\"k\" \"1\" \"other\" \"x\" \"k\" \"2\"").unwrap();
        let keys: Vec<_> = doc.iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["k", "other", "k"]);
        let values: Vec<_> = doc.get_all("k").filter_map(Value::as_str).collect();
        assert_eq!(values, ["1", "2"]);
        assert_eq!(doc.get_str("k"), Some("1"));
    }

    #[test]
    fn lookups_are_ascii_case_insensitive() {
        let doc = parse("\"LibraryFolders\" { \"AppID\" \"1\" }").unwrap();
        let folders = doc.get_object("libraryfolders").unwrap();
        assert_eq!(folders.get_str("appid"), Some("1"));
    }

    #[test]
    fn handles_escapes() {
        let doc =
            parse(r#""path" "C:\\Program Files (x86)\\Steam" "q" "say \"hi\"\tnow\n""#).unwrap();
        assert_eq!(doc.get_str("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(doc.get_str("q"), Some("say \"hi\"\tnow\n"));
    }

    #[test]
    fn unknown_escapes_keep_the_backslash() {
        let doc = parse(r#""path" "C:\Games\x""#).unwrap();
        assert_eq!(doc.get_str("path"), Some(r"C:\Games\x"));
    }

    #[test]
    fn parses_unquoted_tokens() {
        let doc = parse("AppState { appid 228980 name \"Quoted Name\" }").unwrap();
        let state = doc.get_object("AppState").unwrap();
        assert_eq!(state.get_str("appid"), Some("228980"));
        assert_eq!(state.get_str("name"), Some("Quoted Name"));
    }

    #[test]
    fn ignores_comments_but_not_slashes_in_strings() {
        let doc =
            parse("// header\n\"url\" \"https://example.com\" // trailing\n\"a\" \"b\"").unwrap();
        assert_eq!(doc.get_str("url"), Some("https://example.com"));
        assert_eq!(doc.get_str("a"), Some("b"));
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn handles_bom_and_crlf() {
        let doc = parse("\u{feff}\"a\"\r\n{\r\n\t\"b\"\t\t\"c\"\r\n}\r\n").unwrap();
        assert_eq!(doc.get_object("a").unwrap().get_str("b"), Some("c"));
    }

    #[test]
    fn reports_unterminated_strings_at_the_opening_quote() {
        let err = error_of("\"a\" \"b");
        assert_eq!((err.line, err.column), (1, 5));
        assert_eq!(err.kind, ParseErrorKind::UnterminatedString);
    }

    #[test]
    fn rejects_stray_closing_brace() {
        let err = error_of("}");
        assert_eq!(
            (err.line, err.column, err.kind),
            (1, 1, ParseErrorKind::UnexpectedClosingBrace)
        );
    }

    #[test]
    fn rejects_brace_in_key_position() {
        let err = error_of("{");
        assert_eq!(
            (err.line, err.column, err.kind),
            (1, 1, ParseErrorKind::ExpectedKey)
        );
    }

    #[test]
    fn rejects_missing_values() {
        let err = error_of("\"a\"");
        assert_eq!(
            (err.line, err.column, err.kind),
            (1, 4, ParseErrorKind::ExpectedValue)
        );

        let err = error_of("\"a\"\t\"1\"\n\"b\"\t}");
        assert_eq!(
            (err.line, err.column, err.kind),
            (2, 5, ParseErrorKind::ExpectedValue)
        );
    }

    #[test]
    fn rejects_unclosed_objects() {
        let err = error_of("\"a\" {");
        assert_eq!(
            (err.line, err.column, err.kind),
            (1, 6, ParseErrorKind::UnclosedObject)
        );
    }

    #[test]
    fn rejects_unsupported_features() {
        let err = error_of("\"a\" \"b\" [$WIN32]");
        assert_eq!((err.line, err.column), (1, 9));
        assert!(matches!(err.kind, ParseErrorKind::Unsupported(_)));

        let err = error_of("#include \"other.vdf\"");
        assert_eq!((err.line, err.column), (1, 1));
        assert!(matches!(err.kind, ParseErrorKind::Unsupported(_)));
    }

    #[test]
    fn enforces_the_depth_limit() {
        let nested = |depth: usize| format!("{}{}", "\"k\" {".repeat(depth), "}".repeat(depth));
        assert!(parse(&nested(MAX_DEPTH)).is_ok());
        assert_eq!(
            error_of(&nested(MAX_DEPTH + 1)).kind,
            ParseErrorKind::TooDeep
        );
    }

    #[test]
    fn writes_steam_layout() {
        let mut config = Object::new();
        config.push("language", "english");
        let mut state = Object::new();
        state.push("appid", "228980");
        state.push("UserConfig", config);
        let mut doc = Object::new();
        doc.push("AppState", state);

        assert_eq!(
            write(&doc),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"228980\"\n\t\"UserConfig\"\n\t{\n\t\t\"language\"\t\t\"english\"\n\t}\n}\n"
        );
        assert_eq!(doc.to_string(), write(&doc));
    }

    #[test]
    fn writes_only_backslash_and_quote_as_escapes() {
        let mut doc = Object::new();
        doc.push("k", "line one\nline\ttwo \"quoted\" C:\\dir");
        assert_eq!(
            write(&doc),
            "\"k\"\t\t\"line one\nline\ttwo \\\"quoted\\\" C:\\\\dir\"\n"
        );
    }

    #[test]
    fn round_trips_special_characters() {
        let mut doc = Object::new();
        doc.push("path", r"C:\Games\Steam");
        doc.push(
            "quote \"inside\" key",
            "tab\tnewline\nbackslash\\ unicode äöü 🎮",
        );
        doc.push("empty", "");
        doc.push("nested", Object::new());
        assert_eq!(parse(&write(&doc)).unwrap(), doc);
    }

    #[test]
    fn insert_replaces_first_match_and_keeps_spelling() {
        let mut doc = parse("\"StateFlags\" \"2\" \"name\" \"x\"").unwrap();
        assert_eq!(doc.insert("stateflags", "4"), Some(Value::from("2")));
        assert_eq!(doc.iter().next(), Some(("StateFlags", &Value::from("4"))));
        assert_eq!(doc.insert("new", "1"), None);
        assert_eq!(doc.len(), 3);
        assert_eq!(doc.remove("NAME"), Some(Value::from("x")));
        assert_eq!(doc.len(), 2);
    }
}
