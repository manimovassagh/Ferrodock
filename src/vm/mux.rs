//! Many connections over one byte stream: how Ferrodock talks to the
//! engine's agent on Windows, over the standard input and output of the
//! WSL process it runs in. Shared with the agent (`agent/src/stdio.rs`).
//!
//! Every frame is a 9-byte header, the connection's id (u32), its kind
//! (u8) and the data's length (u32), all big-endian, then the data.

use std::io::{self, Read};

/// A new connection to the engine (Ferrodock → agent).
pub const OPEN: u8 = 1;
/// Bytes on a connection, either way.
pub const DATA: u8 = 2;
/// The sender has nothing more to send on this connection.
pub const CLOSE: u8 = 3;

pub const HEADER: usize = 9;
/// The most data one frame carries.
pub const MAX_DATA: usize = 1 << 20;

/// One frame, ready to write.
pub fn encode(id: u32, kind: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER + data.len());
    frame.extend_from_slice(&id.to_be_bytes());
    frame.push(kind);
    frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

/// The id, kind and data length in a header.
pub fn header(bytes: &[u8; HEADER]) -> io::Result<(u32, u8, usize)> {
    let id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let len = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]) as usize;
    if !(OPEN..=CLOSE).contains(&bytes[4]) || len > MAX_DATA {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad frame"));
    }
    Ok((id, bytes[4], len))
}

/// Reads the next frame: (id, kind, data). None when the stream ended
/// between frames.
pub fn read(input: &mut impl Read) -> io::Result<Option<(u32, u8, Vec<u8>)>> {
    let mut bytes = [0u8; HEADER];
    let mut got = 0;
    while got < HEADER {
        match input.read(&mut bytes[got..])? {
            0 if got == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => got += n,
        }
    }
    let (id, kind, len) = header(&bytes)?;
    let mut data = vec![0u8; len];
    input.read_exact(&mut data)?;
    Ok(Some((id, kind, data)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut stream = encode(7, OPEN, b"");
        stream.extend(encode(7, DATA, b"GET /_ping HTTP/1.1\r\n\r\n"));
        stream.extend(encode(u32::MAX, CLOSE, b""));
        let mut input = stream.as_slice();
        assert_eq!(read(&mut input).unwrap(), Some((7, OPEN, vec![])));
        assert_eq!(
            read(&mut input).unwrap(),
            Some((7, DATA, b"GET /_ping HTTP/1.1\r\n\r\n".to_vec()))
        );
        assert_eq!(read(&mut input).unwrap(), Some((u32::MAX, CLOSE, vec![])));
        assert_eq!(read(&mut input).unwrap(), None);
        // Cut short, or nonsense: an error, never a wrong frame.
        assert!(read(&mut &encode(1, DATA, b"abc")[..10]).is_err());
        assert!(read(&mut &[0, 0, 0, 1, 9, 0, 0, 0, 0][..]).is_err());
    }

    /// Hands out one byte per read, like a pipe under load.
    struct Trickle<'a>(&'a [u8]);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let Some((first, rest)) = self.0.split_first() else {
                return Ok(0);
            };
            buf[0] = *first;
            self.0 = rest;
            Ok(1)
        }
    }

    #[test]
    fn reads_frames_that_arrive_a_byte_at_a_time() {
        let mut stream = encode(3, DATA, b"hello");
        stream.extend(encode(4, CLOSE, b""));
        let mut input = Trickle(&stream);
        assert_eq!(
            read(&mut input).unwrap(),
            Some((3, DATA, b"hello".to_vec()))
        );
        assert_eq!(read(&mut input).unwrap(), Some((4, CLOSE, vec![])));
        assert_eq!(read(&mut input).unwrap(), None);
    }

    #[test]
    fn caps_the_frame_size() {
        let mut at_most = [0u8; HEADER];
        at_most.copy_from_slice(&encode(1, DATA, b"")[..HEADER]);
        at_most[5..].copy_from_slice(&(MAX_DATA as u32).to_be_bytes());
        assert_eq!(header(&at_most).unwrap(), (1, DATA, MAX_DATA));
        // One byte more is refused before anything is allocated for it.
        let mut over = at_most;
        over[5..].copy_from_slice(&(MAX_DATA as u32 + 1).to_be_bytes());
        assert!(header(&over).is_err());
        assert!(read(&mut &over[..]).is_err());
    }

    #[test]
    fn a_stream_cut_mid_header_is_an_error() {
        let frame = encode(9, OPEN, b"");
        for cut in 1..HEADER {
            let error = read(&mut &frame[..cut]).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        }
    }
}
