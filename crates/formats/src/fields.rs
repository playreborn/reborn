//! Helpers for reading typed fields out of a [`vdf::Object`] with
//! error messages that name the full key path.

use std::str::FromStr;

use crate::Error;
use crate::vdf::{Object, Value};

/// Joins a parent path and a key: `("AppState", "appid")` → `AppState/appid`.
pub(crate) fn key_path(context: &str, key: &str) -> String {
    if context.is_empty() {
        key.to_owned()
    } else {
        format!("{context}/{key}")
    }
}

/// The object stored under `key`.
pub(crate) fn section<'a>(
    parent: &'a Object,
    context: &str,
    key: &str,
) -> Result<&'a Object, Error> {
    match parent.get(key) {
        Some(Value::Object(object)) => Ok(object),
        Some(Value::String(_)) => Err(Error::WrongType {
            key: key_path(context, key),
            expected: "an object",
        }),
        None => Err(Error::MissingKey(key_path(context, key))),
    }
}

/// The string stored under `key`, if present.
pub(crate) fn optional_str<'a>(
    object: &'a Object,
    context: &str,
    key: &str,
) -> Result<Option<&'a str>, Error> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(Value::Object(_)) => Err(Error::WrongType {
            key: key_path(context, key),
            expected: "a string",
        }),
    }
}

/// The string stored under `key`.
pub(crate) fn required_str<'a>(
    object: &'a Object,
    context: &str,
    key: &str,
) -> Result<&'a str, Error> {
    optional_str(object, context, key)?.ok_or_else(|| Error::MissingKey(key_path(context, key)))
}

/// The number stored under `key`, if present. An empty string counts as absent.
pub(crate) fn optional_number<T: FromStr>(
    object: &Object,
    context: &str,
    key: &str,
) -> Result<Option<T>, Error> {
    match optional_str(object, context, key)? {
        None => Ok(None),
        Some(text) => parse_number(text, &key_path(context, key)),
    }
}

/// The number stored under `key`.
pub(crate) fn required_number<T: FromStr>(
    object: &Object,
    context: &str,
    key: &str,
) -> Result<T, Error> {
    optional_number(object, context, key)?.ok_or_else(|| Error::MissingKey(key_path(context, key)))
}

/// Parses a number; an empty (or whitespace-only) string yields `None`.
pub(crate) fn parse_number<T: FromStr>(text: &str, path: &str) -> Result<Option<T>, Error> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed.parse().map(Some).map_err(|_| Error::InvalidNumber {
        key: path.to_owned(),
        value: text.to_owned(),
    })
}
