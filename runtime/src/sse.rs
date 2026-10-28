//! Incremental UTF-8 SSE decoding shared by all text event-stream adapters.
use crate::Failure;

#[derive(Default)]
pub(crate) struct Decoder {
    pending: Vec<u8>,
    data: Vec<String>,
    started: bool,
}
impl Decoder {
    pub fn feed(&mut self, bytes: &[u8], eof: bool) -> Result<Vec<String>, Failure> {
        self.pending.extend_from_slice(bytes);
        if !self.started {
            if self.pending.is_empty() && !eof {
                return Ok(Vec::new());
            }
            if self.pending.first() == Some(&0xef) && self.pending.len() < 3 && !eof {
                return Ok(Vec::new());
            }
            if self.pending.starts_with(&[0xef, 0xbb, 0xbf]) {
                self.pending.drain(..3);
            }
            self.started = true;
        }
        let mut events = Vec::new();
        let mut consumed = 0;
        while let Some(relative) = self.pending[consumed..]
            .iter()
            .position(|b| *b == b'\r' || *b == b'\n')
        {
            let end = consumed + relative;
            if self.pending[end] == b'\r' && end + 1 == self.pending.len() && !eof {
                break; // The LF of a CRLF pair may arrive in the next chunk.
            }
            let terminator =
                if self.pending[end] == b'\r' && self.pending.get(end + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            let line = std::str::from_utf8(&self.pending[consumed..end])
                .map_err(|_| Failure::new(502, "invalid_stream_encoding"))?;
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).to_owned());
            } else if line == "data" {
                self.data.push(String::new());
            }
            consumed = end + terminator;
        }
        self.pending.drain(..consumed);
        // An unterminated event at EOF is deliberately not dispatched.
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bom_unicode_and_all_line_endings_survive_every_split() {
        for separator in ["\n", "\r", "\r\n"] {
            let wire = format!(
                "\u{feff}data: héllo 🦀{separator}{separator}data: [DONE]{separator}{separator}"
            );
            for split in 0..=wire.len() {
                let mut decoder = Decoder::default();
                let mut events = decoder.feed(&wire.as_bytes()[..split], false).unwrap();
                events.extend(decoder.feed(&wire.as_bytes()[split..], true).unwrap());
                assert_eq!(events, vec!["héllo 🦀", "[DONE]"]);
            }
            let mut decoder = Decoder::default();
            let mut events = Vec::new();
            for byte in wire.bytes() {
                events.extend(decoder.feed(&[byte], false).unwrap());
            }
            events.extend(decoder.feed(&[], true).unwrap());
            assert_eq!(events, vec!["héllo 🦀", "[DONE]"]);
        }
    }
    #[test]
    fn multiline_comments_and_unterminated_data() {
        let mut decoder = Decoder::default();
        assert_eq!(
            decoder
                .feed(b": ping\rdata: one\rdata: two\r\rdata: unfinished", true)
                .unwrap(),
            vec!["one\ntwo"]
        );
    }
}
