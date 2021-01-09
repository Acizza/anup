use anyhow::{anyhow, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{
    de::{self, Deserializer, Visitor},
    Deserialize, Serialize, Serializer,
};
use smallvec::SmallVec;
use std::{
    convert::{TryFrom, TryInto},
    ops::Deref,
    result,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key(KeyEvent);

impl Key {
    pub fn new(event: KeyEvent) -> Self {
        Self(event)
    }

    pub fn from_code(code: KeyCode) -> Self {
        Self(KeyEvent::new(code, KeyModifiers::NONE))
    }

    pub fn ctrl_pressed(&self) -> bool {
        self.0.modifiers.contains(KeyModifiers::CONTROL)
    }
}

impl Deref for Key {
    type Target = KeyCode;

    fn deref(&self) -> &Self::Target {
        &self.0.code
    }
}

impl TryFrom<&str> for Key {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value = value.to_ascii_lowercase();
        let modifier_split = value
            .splitn(2, '+')
            .map(str::trim)
            .collect::<SmallVec<[_; 2]>>();

        let (modifier, key) = match modifier_split.as_slice() {
            ["ctrl", key] => (KeyModifiers::CONTROL, key),
            ["shift", key] => (KeyModifiers::SHIFT, key),
            ["alt", key] => (KeyModifiers::ALT, key),
            [_, key] | [key] => (KeyModifiers::NONE, key),
            [] => return Err(anyhow!("no key specified")),
            _ => return Err(anyhow!("malformed key")),
        };

        let code = match *key {
            "backspace" => KeyCode::Backspace,
            "enter" => KeyCode::Enter,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "delete" => KeyCode::Delete,
            "insert" => KeyCode::Insert,
            "unknown" => KeyCode::Null,
            "escape" => KeyCode::Esc,
            key if key.len() == 1 && key.is_ascii() => {
                let bytes = key.as_bytes();
                KeyCode::Char(bytes[0] as char)
            }
            key if key.starts_with('f') => {
                let num = key[1..].parse().context("invalid F key")?;

                if !(1..=12).contains(&num) {
                    return Err(anyhow!("F key must be between 1-12"));
                }

                KeyCode::F(num)
            }
            unknown => return Err(anyhow!("unknown key: {}", unknown)),
        };

        let event = KeyEvent::new(code, modifier);

        Ok(Self(event))
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(de: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use std::fmt;

        struct KeyVisitor;

        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = Key;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a key")
            }

            fn visit_str<E>(self, value: &str) -> result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                value.try_into().map_err(E::custom)
            }
        }

        de.deserialize_str(KeyVisitor)
    }
}

impl Serialize for Key {
    fn serialize<S>(&self, se: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0.code {
            KeyCode::Backspace => se.serialize_str("backspace"),
            KeyCode::Enter => se.serialize_str("enter"),
            KeyCode::Left => se.serialize_str("left"),
            KeyCode::Right => se.serialize_str("right"),
            KeyCode::Up => se.serialize_str("up"),
            KeyCode::Down => se.serialize_str("down"),
            KeyCode::Home => se.serialize_str("home"),
            KeyCode::End => se.serialize_str("end"),
            KeyCode::PageUp => se.serialize_str("pageup"),
            KeyCode::PageDown => se.serialize_str("pagedown"),
            KeyCode::Tab => se.serialize_str("tab"),
            KeyCode::BackTab => se.serialize_str("backtab"),
            KeyCode::Delete => se.serialize_str("delete"),
            KeyCode::Insert => se.serialize_str("insert"),
            KeyCode::F(key) => se.serialize_str(&format!("f{}", key)),
            KeyCode::Char(key) => se.serialize_char(key),
            KeyCode::Null => se.serialize_str("unknown"),
            KeyCode::Esc => se.serialize_str("escape"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Key;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::convert::TryInto;

    macro_rules! test_key {
        ($key:expr, $expected_code:expr => $modifier:expr) => {{
            let value = $key.try_into().map_err(|e: anyhow::Error| e.to_string());
            let expected = Key::new(KeyEvent::new($expected_code, $modifier));
            assert_eq!(value, Ok(expected));
        }};
    }

    #[test]
    fn valid_keys() {
        test_key!("j", KeyCode::Char('j') => KeyModifiers::NONE);
        test_key!("f1", KeyCode::F(1) => KeyModifiers::NONE);
        test_key!("K", KeyCode::Char('k') => KeyModifiers::NONE);
        test_key!("ctrl+b", KeyCode::Char('b') => KeyModifiers::CONTROL);
        test_key!("CTRL+B", KeyCode::Char('b') => KeyModifiers::CONTROL);
        test_key!("shift+enter", KeyCode::Enter => KeyModifiers::SHIFT);
        test_key!("alt+tab", KeyCode::Tab => KeyModifiers::ALT);
        test_key!("ctrl + backspace", KeyCode::Backspace => KeyModifiers::CONTROL);
        test_key!("  shift +  f12", KeyCode::F(12) => KeyModifiers::SHIFT);
        test_key!("f1", KeyCode::F(1) => KeyModifiers::NONE);
    }

    #[test]
    #[should_panic]
    fn invalid_keys() {
        test_key!("a", KeyCode::Char('b') => KeyModifiers::NONE);
        test_key!("j", KeyCode::Char('j') => KeyModifiers::CONTROL);
        test_key!("f0", KeyCode::F(0) => KeyModifiers::NONE);
        test_key!("f13", KeyCode::F(13) => KeyModifiers::NONE);
        test_key!("ctrl++a", KeyCode::Char('a') => KeyModifiers::CONTROL);
        test_key!("shift", KeyCode::Null => KeyModifiers::SHIFT);
        test_key!("", KeyCode::Null => KeyModifiers::NONE);
    }
}
