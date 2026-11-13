//! `aws-chunked` bodies: how the CLI and SDKs send an upload they sign or
//! checksum as it goes. The object's bytes come in chunks, each after a
//! line giving its length (and a signature, which is ignored), with
//! trailers at the end:
//!
//! ```text
//! 5;chunk-signature=…\r\nhello\r\n0;chunk-signature=…\r\nx-amz-checksum-crc32:…\r\n\r\n
//! ```

/// The longest a chunk's header line may be.
const MAX_LINE: usize = 4096;

enum State {
    /// Reading a chunk's header line.
    Line(Vec<u8>),
    /// Reading a chunk's bytes: how many are left.
    Data(u64),
    /// After a chunk's bytes: its closing `\r\n`, as far as it's been seen.
    End(u8),
    /// After the last chunk: trailers, which aren't needed.
    Trailers,
}

/// Takes the framing off, piece by piece.
pub struct Decoder {
    state: State,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            state: State::Line(Vec::new()),
        }
    }
}

impl Decoder {
    /// Takes the next piece of the body as sent, and adds the object's
    /// bytes in it to `out`.
    pub fn feed(&mut self, mut input: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
        while !input.is_empty() {
            match &mut self.state {
                State::Line(line) => {
                    let (byte, rest) = (input[0], &input[1..]);
                    input = rest;
                    if byte != b'\n' {
                        line.push(byte);
                        if line.len() > MAX_LINE {
                            return Err("a chunk's header is too long".into());
                        }
                        continue;
                    }
                    let text = String::from_utf8_lossy(line);
                    let size = text
                        .trim_end_matches('\r')
                        .split(';')
                        .next()
                        .unwrap_or_default();
                    let size = u64::from_str_radix(size.trim(), 16)
                        .map_err(|_| format!("a chunk's length isn't a number: {size}"))?;
                    self.state = if size == 0 {
                        State::Trailers
                    } else {
                        State::Data(size)
                    };
                }
                State::Data(left) => {
                    let take = (*left).min(input.len() as u64) as usize;
                    out.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    *left -= take as u64;
                    if *left == 0 {
                        self.state = State::End(0);
                    }
                }
                State::End(seen) => {
                    let expected = if *seen == 0 { b'\r' } else { b'\n' };
                    if input[0] != expected {
                        return Err("a chunk doesn't end where its length says".into());
                    }
                    input = &input[1..];
                    self.state = if *seen == 0 {
                        State::End(1)
                    } else {
                        State::Line(Vec::new())
                    };
                }
                State::Trailers => return Ok(()),
            }
        }
        Ok(())
    }

    /// Whether the last chunk has been seen: a body that ends before it
    /// was cut short.
    pub fn complete(&self) -> bool {
        matches!(self.state, State::Trailers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &[u8] = b"5;chunk-signature=abc\r\nhello\r\n6;chunk-signature=def\r\n world\r\n0;chunk-signature=fin\r\nx-amz-checksum-crc32:AAAA\r\n\r\n";

    #[test]
    fn takes_the_framing_off() {
        let mut decoder = Decoder::default();
        let mut out = Vec::new();
        decoder.feed(BODY, &mut out).unwrap();
        assert_eq!(out, b"hello world");
        assert!(decoder.complete());
    }

    #[test]
    fn works_whatever_the_pieces() {
        for size in [1, 2, 3, 7, 16] {
            let mut decoder = Decoder::default();
            let mut out = Vec::new();
            for piece in BODY.chunks(size) {
                decoder.feed(piece, &mut out).unwrap();
            }
            assert_eq!(out, b"hello world", "pieces of {size}");
            assert!(decoder.complete());
        }
    }

    #[test]
    fn notices_a_broken_body() {
        let mut out = Vec::new();
        assert!(Decoder::default().feed(b"zz\r\nhello", &mut out).is_err());
        assert!(
            Decoder::default()
                .feed(b"2\r\nhello\r\n", &mut out)
                .is_err()
        );
        let mut cut = Decoder::default();
        cut.feed(b"5\r\nhel", &mut out).unwrap();
        assert!(!cut.complete());
    }
}
