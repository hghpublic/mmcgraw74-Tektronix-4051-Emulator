//! Lossless JSON syntax for Flash Drive archive headers.
//!
//! Strings decode directly into UTF-16 units, retaining escaped unpaired
//! surrogates from the web format. Encoding preserves those units while
//! emitting valid surrogate pairs as Unicode scalars. Tape metadata validation,
//! ZIP streams, storage state and host file operations belong to the callers.

#[cfg(test)]
mod tests;

pub(crate) struct ArchiveHeaderParser<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> ArchiveHeaderParser<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    pub(crate) fn consume(&mut self, expected: u8) -> Result<(), String> {
        self.skip_whitespace();
        if self.source.as_bytes().get(self.offset) == Some(&expected) {
            self.offset += 1;
            Ok(())
        } else {
            Err(format!(
                "archive header expected {:?} at byte {}",
                char::from(expected),
                self.offset
            ))
        }
    }

    pub(crate) fn parse_number(&mut self) -> Result<u16, String> {
        self.skip_whitespace();
        let start = self.offset;
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_digit)
        {
            self.offset += 1;
        }
        if start == self.offset
            || (self.source.as_bytes()[start] == b'0' && self.offset.saturating_sub(start) > 1)
        {
            return Err(format!(
                "archive header has an invalid file number at byte {start}"
            ));
        }
        self.source[start..self.offset]
            .parse()
            .map_err(|error| format!("archive header has an invalid file number: {error}"))
    }

    pub(crate) fn parse_string(&mut self) -> Result<Vec<u16>, String> {
        self.skip_whitespace();
        if self.source.as_bytes().get(self.offset) != Some(&b'"') {
            return Err(format!(
                "archive header expected a JSON string at byte {}",
                self.offset
            ));
        }
        self.offset += 1;
        let mut units = Vec::new();
        while let Some(&byte) = self.source.as_bytes().get(self.offset) {
            match byte {
                b'"' => {
                    self.offset += 1;
                    return Ok(units);
                }
                b'\\' => {
                    self.offset += 1;
                    let escape = *self
                        .source
                        .as_bytes()
                        .get(self.offset)
                        .ok_or_else(|| "archive header ends in a JSON escape".to_owned())?;
                    self.offset += 1;
                    match escape {
                        b'"' | b'\\' | b'/' => units.push(u16::from(escape)),
                        b'b' => units.push(0x0008),
                        b'f' => units.push(0x000c),
                        b'n' => units.push(0x000a),
                        b'r' => units.push(0x000d),
                        b't' => units.push(0x0009),
                        b'u' => units.push(self.parse_unicode_escape()?),
                        _ => {
                            return Err("archive header contains an invalid JSON escape".to_owned());
                        }
                    }
                }
                0x00..=0x1f => {
                    return Err(
                        "archive header contains an unescaped JSON control character".to_owned(),
                    );
                }
                0x20..=0x7f => {
                    units.push(u16::from(byte));
                    self.offset += 1;
                }
                _ => {
                    let character = self.source[self.offset..]
                        .chars()
                        .next()
                        .ok_or_else(|| "archive header contains invalid UTF-8".to_owned())?;
                    let mut encoded = [0; 2];
                    units.extend_from_slice(character.encode_utf16(&mut encoded));
                    self.offset += character.len_utf8();
                }
            }
        }
        Err("archive header contains an unterminated JSON string".to_owned())
    }

    fn parse_unicode_escape(&mut self) -> Result<u16, String> {
        let end = self.offset.saturating_add(4);
        let digits = self
            .source
            .as_bytes()
            .get(self.offset..end)
            .ok_or_else(|| "archive header has an incomplete Unicode escape".to_owned())?;
        if !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err("archive header has an invalid Unicode escape".to_owned());
        }
        self.offset = end;
        let digits = self
            .source
            .get(end - 4..end)
            .expect("validated ASCII hex digits form UTF-8");
        u16::from_str_radix(digits, 16)
            .map_err(|error| format!("archive header has an invalid Unicode escape: {error}"))
    }

    pub(crate) fn finish(&mut self) -> Result<(), String> {
        self.skip_whitespace();
        if self.offset == self.source.len() {
            Ok(())
        } else {
            Err(format!(
                "archive header has trailing JSON data at byte {}",
                self.offset
            ))
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.offset += 1;
        }
    }
}

pub(crate) fn json_utf16_string(units: &[u16]) -> String {
    let mut encoded = String::with_capacity(units.len() + 2);
    encoded.push('"');
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        match unit {
            0x0022 => encoded.push_str("\\\""),
            0x005c => encoded.push_str("\\\\"),
            0x0008 => encoded.push_str("\\b"),
            0x0009 => encoded.push_str("\\t"),
            0x000a => encoded.push_str("\\n"),
            0x000c => encoded.push_str("\\f"),
            0x000d => encoded.push_str("\\r"),
            0x0000..=0x001f => push_json_unicode_escape(&mut encoded, unit),
            0xd800..=0xdbff => {
                if let Some(&low) = units.get(index + 1)
                    && (0xdc00..=0xdfff).contains(&low)
                {
                    let scalar =
                        0x1_0000 + (u32::from(unit - 0xd800) << 10) + u32::from(low - 0xdc00);
                    encoded.push(
                        char::from_u32(scalar).expect("a valid UTF-16 pair is a Unicode scalar"),
                    );
                    index += 2;
                    continue;
                }
                push_json_unicode_escape(&mut encoded, unit);
            }
            0xdc00..=0xdfff => push_json_unicode_escape(&mut encoded, unit),
            _ => encoded.push(
                char::from_u32(u32::from(unit)).expect("a non-surrogate UTF-16 unit is a scalar"),
            ),
        }
        index += 1;
    }
    encoded.push('"');
    encoded
}

fn push_json_unicode_escape(encoded: &mut String, unit: u16) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    encoded.push_str("\\u");
    for shift in [12, 8, 4, 0] {
        let digit = usize::from((unit >> shift) & 0x0f);
        encoded.push(char::from(HEX[digit]));
    }
}
