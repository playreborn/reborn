//! Binary KeyValues, as used in PICS package info, `appinfo.vdf` entries
//! and other places where Steam stores KeyValues compactly.
//!
//! # Format
//!
//! A sequence of entries, each `type (u8)`, `key (NUL-terminated UTF-8)`,
//! then a value depending on the type. An object ends with an end marker.
//! All numbers are little-endian.
//!
//! | Type | Meaning | Value |
//! | --- | --- | --- |
//! | 0 | object | nested entries up to an end marker |
//! | 1 | string | NUL-terminated UTF-8 |
//! | 2 | int32 | 4 bytes |
//! | 3 | float32 | 4 bytes |
//! | 4 | pointer | 4 bytes, read as int32 |
//! | 5 | wide string | not supported |
//! | 6 | color | 4 bytes, read as int32 |
//! | 7 | uint64 | 8 bytes |
//! | 8, 11 | end of object | none |
//! | 10 | int64 | 8 bytes |
//!
//! Source: SteamKit2 `SteamKit2/Types/KeyValue.cs`, `TryReadAsBinaryCore`
//! (types, byte order, and numbers converted to decimal strings).
//!
//! Numbers are converted to decimal strings, so the result is the same
//! [`Object`] type as the text format and can be read with the same helpers.
//! This is a reader only; Reborn does not need to write binary KeyValues yet.

use crate::vdf::{MAX_DEPTH, Object};

const TYPE_OBJECT: u8 = 0;
const TYPE_STRING: u8 = 1;
const TYPE_INT32: u8 = 2;
const TYPE_FLOAT32: u8 = 3;
const TYPE_POINTER: u8 = 4;
const TYPE_WIDE_STRING: u8 = 5;
const TYPE_COLOR: u8 = 6;
const TYPE_UINT64: u8 = 7;
const TYPE_END: u8 = 8;
const TYPE_INT64: u8 = 10;
const TYPE_ALTERNATE_END: u8 = 11;

/// Error returned by [`parse`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at byte offset {offset}")]
pub struct BinaryError {
    /// Byte offset where the problem was detected.
    pub offset: usize,
    /// What went wrong.
    pub kind: BinaryErrorKind,
}

/// The kind of a [`BinaryError`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BinaryErrorKind {
    /// The input ends in the middle of an entry or inside an object.
    #[error("unexpected end of input")]
    UnexpectedEof,
    /// A key or string value is not valid UTF-8.
    #[error("string is not valid UTF-8")]
    InvalidUtf8,
    /// An unknown type byte.
    #[error("unknown value type {0}")]
    UnknownType(u8),
    /// Wide (UTF-16) strings are not supported, like in SteamKit2.
    #[error("wide strings are not supported")]
    WideString,
    /// Objects are nested deeper than [`MAX_DEPTH`].
    #[error("objects are nested deeper than {} levels", MAX_DEPTH)]
    TooDeep,
}

/// Parses binary KeyValues into an [`Object`] holding the top-level entries.
///
/// Parsing stops at the end marker of the top-level object; anything after
/// it is ignored. The end of the input also ends the top-level object.
pub fn parse(input: &[u8]) -> Result<Object, BinaryError> {
    let mut reader = Reader { input, pos: 0 };
    let mut open: Vec<(String, Object)> = Vec::new();
    let mut current = Object::new();

    loop {
        let type_offset = reader.pos;
        let Some(value_type) = reader.byte() else {
            if open.is_empty() {
                return Ok(current);
            }
            return Err(reader.error_at(type_offset, BinaryErrorKind::UnexpectedEof));
        };

        if matches!(value_type, TYPE_END | TYPE_ALTERNATE_END) {
            match open.pop() {
                Some((key, mut parent)) => {
                    parent.push(key, std::mem::take(&mut current));
                    current = parent;
                    continue;
                }
                None => return Ok(current),
            }
        }

        let key = reader.string()?;
        match value_type {
            TYPE_OBJECT => {
                if open.len() >= MAX_DEPTH {
                    return Err(reader.error_at(type_offset, BinaryErrorKind::TooDeep));
                }
                open.push((key, std::mem::take(&mut current)));
            }
            TYPE_STRING => {
                let value = reader.string()?;
                current.push(key, value);
            }
            TYPE_INT32 | TYPE_POINTER | TYPE_COLOR => {
                let value = i32::from_le_bytes(reader.array()?);
                current.push(key, value.to_string());
            }
            TYPE_FLOAT32 => {
                let value = f32::from_le_bytes(reader.array()?);
                current.push(key, value.to_string());
            }
            TYPE_UINT64 => {
                let value = u64::from_le_bytes(reader.array()?);
                current.push(key, value.to_string());
            }
            TYPE_INT64 => {
                let value = i64::from_le_bytes(reader.array()?);
                current.push(key, value.to_string());
            }
            TYPE_WIDE_STRING => {
                return Err(reader.error_at(type_offset, BinaryErrorKind::WideString));
            }
            other => {
                return Err(reader.error_at(type_offset, BinaryErrorKind::UnknownType(other)));
            }
        }
    }
}

struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn error_at(&self, offset: usize, kind: BinaryErrorKind) -> BinaryError {
        BinaryError { offset, kind }
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.input.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], BinaryError> {
        let bytes = self
            .input
            .get(self.pos..self.pos + N)
            .ok_or(self.error_at(self.pos, BinaryErrorKind::UnexpectedEof))?;
        self.pos += N;
        let mut array = [0; N];
        array.copy_from_slice(bytes);
        Ok(array)
    }

    /// Reads a NUL-terminated UTF-8 string.
    fn string(&mut self) -> Result<String, BinaryError> {
        let start = self.pos;
        let rest = &self.input[start..];
        let length = rest
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(self.error_at(start, BinaryErrorKind::UnexpectedEof))?;
        let text = std::str::from_utf8(&rest[..length])
            .map_err(|_| self.error_at(start, BinaryErrorKind::InvalidUtf8))?;
        self.pos += length + 1;
        Ok(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small builder for test input.
    #[derive(Default)]
    struct Bytes(Vec<u8>);

    impl Bytes {
        fn entry(mut self, value_type: u8, key: &str) -> Self {
            self.0.push(value_type);
            self.0.extend_from_slice(key.as_bytes());
            self.0.push(0);
            self
        }
        fn raw(mut self, bytes: &[u8]) -> Self {
            self.0.extend_from_slice(bytes);
            self
        }
        fn string(self, key: &str, value: &str) -> Self {
            self.entry(TYPE_STRING, key).raw(value.as_bytes()).raw(&[0])
        }
        fn int32(self, key: &str, value: i32) -> Self {
            self.entry(TYPE_INT32, key).raw(&value.to_le_bytes())
        }
        fn object(self, key: &str) -> Self {
            self.entry(TYPE_OBJECT, key)
        }
        fn end(self) -> Self {
            self.raw(&[TYPE_END])
        }
    }

    #[test]
    fn parses_a_pics_style_package() {
        // Shape of a PICS package info body (after the 4-byte format header).
        let input = Bytes::default()
            .object("12345")
            .int32("packageid", 12345)
            .int32("billingtype", 10)
            .object("appids")
            .int32("0", 730)
            .int32("1", 228980)
            .end()
            .end()
            .end()
            .0;

        let doc = parse(&input).unwrap();
        let package = doc.get_object("12345").unwrap();
        assert_eq!(package.get_str("packageid"), Some("12345"));
        let apps: Vec<_> = package
            .get_object("appids")
            .unwrap()
            .iter()
            .filter_map(|(_, value)| value.as_str())
            .collect();
        assert_eq!(apps, ["730", "228980"]);
    }

    #[test]
    fn converts_all_number_types() {
        let input = Bytes::default()
            .int32("i32", -5)
            .entry(TYPE_FLOAT32, "f32")
            .raw(&1.5f32.to_le_bytes())
            .entry(TYPE_UINT64, "u64")
            .raw(&u64::MAX.to_le_bytes())
            .entry(TYPE_INT64, "i64")
            .raw(&i64::MIN.to_le_bytes())
            .entry(TYPE_COLOR, "color")
            .raw(&7i32.to_le_bytes())
            .string("text", "äöü")
            .end()
            .0;

        let doc = parse(&input).unwrap();
        assert_eq!(doc.get_str("i32"), Some("-5"));
        assert_eq!(doc.get_str("f32"), Some("1.5"));
        assert_eq!(doc.get_str("u64"), Some("18446744073709551615"));
        assert_eq!(doc.get_str("i64"), Some("-9223372036854775808"));
        assert_eq!(doc.get_str("color"), Some("7"));
        assert_eq!(doc.get_str("text"), Some("äöü"));
    }

    #[test]
    fn accepts_end_of_input_as_top_level_end_and_alternate_end() {
        let without_end = Bytes::default().string("a", "b").0;
        assert_eq!(parse(&without_end).unwrap().get_str("a"), Some("b"));

        let alternate = Bytes::default()
            .object("o")
            .string("a", "b")
            .raw(&[TYPE_ALTERNATE_END])
            .0;
        assert_eq!(
            parse(&alternate)
                .unwrap()
                .get_object("o")
                .unwrap()
                .get_str("a"),
            Some("b")
        );
    }

    #[test]
    fn ignores_data_after_the_top_level_end() {
        let input = Bytes::default().string("a", "b").end().raw(&[0xff, 0xff]).0;
        assert_eq!(parse(&input).unwrap().len(), 1);
    }

    #[test]
    fn reports_truncated_input() {
        let truncated_number = Bytes::default().entry(TYPE_INT32, "n").raw(&[1, 2]).0;
        assert_eq!(
            parse(&truncated_number).unwrap_err(),
            BinaryError {
                offset: 3,
                kind: BinaryErrorKind::UnexpectedEof
            }
        );

        let unterminated_key = Bytes::default().raw(&[TYPE_STRING, b'k']).0;
        assert_eq!(
            parse(&unterminated_key).unwrap_err().kind,
            BinaryErrorKind::UnexpectedEof
        );

        let unclosed_object = Bytes::default().object("o").string("a", "b").0;
        assert_eq!(
            parse(&unclosed_object).unwrap_err().kind,
            BinaryErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn rejects_unknown_types_wide_strings_and_bad_utf8() {
        let unknown = Bytes::default().entry(9, "x").0;
        assert_eq!(
            parse(&unknown).unwrap_err(),
            BinaryError {
                offset: 0,
                kind: BinaryErrorKind::UnknownType(9)
            }
        );

        let wide = Bytes::default().entry(TYPE_WIDE_STRING, "x").0;
        assert_eq!(parse(&wide).unwrap_err().kind, BinaryErrorKind::WideString);

        let bad_utf8 = Bytes::default().raw(&[TYPE_STRING, 0xff, 0]).0;
        assert_eq!(
            parse(&bad_utf8).unwrap_err().kind,
            BinaryErrorKind::InvalidUtf8
        );
    }

    #[test]
    fn enforces_the_depth_limit() {
        let nested = |depth: usize| {
            let mut bytes = Bytes::default();
            for _ in 0..depth {
                bytes = bytes.object("k");
            }
            for _ in 0..depth {
                bytes = bytes.end();
            }
            bytes.0
        };
        assert!(parse(&nested(MAX_DEPTH)).is_ok());
        assert_eq!(
            parse(&nested(MAX_DEPTH + 1)).unwrap_err().kind,
            BinaryErrorKind::TooDeep
        );
    }
}
