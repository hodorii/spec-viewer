//! Selection-copy destination (design.md "app/clipboard.rs # Clipboard
//! trait; Osc52 impl, test sink"; requirement 9.9). Kept behind a trait so
//! the reducer can call it directly and synchronously (design.md Key
//! Decisions: "Selection copy via OSC 52 ... Clipboard 트레이트 뒤에 두어
//! 테스트는 싱크로 대체") without any test needing a real terminal.
//!
//! No clipboard crate is used (design.md Technology Stack: "크레이트
//! 없음") -- [`Osc52`] writes the escape sequence itself, base64-encoding
//! the payload with a small local encoder rather than pulling in a
//! dependency for it.

use std::cell::RefCell;
use std::rc::Rc;

/// Destination for a copied selection's text.
pub trait Clipboard {
    fn set(&mut self, text: &str);
}

/// Copies via the terminal OSC 52 escape sequence (`ESC ] 52 ; c ; <base64>
/// BEL`) -- the "c" target selects the system clipboard. A terminal that
/// does not understand OSC 52 simply ignores the unrecognized sequence
/// (requirement 9.9's "지원 안 하는 터미널은 조용히 무시"), so `set` never
/// needs to detect support itself.
pub struct Osc52;

impl Clipboard for Osc52 {
    fn set(&mut self, text: &str) {
        use std::io::Write;
        let payload = base64_encode(text.as_bytes());
        let _ = write!(std::io::stdout(), "\x1b]52;c;{payload}\x07");
        let _ = std::io::stdout().flush();
    }
}

/// Standard (RFC 4648) base64 alphabet, `=`-padded.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Test double: records the last copied text instead of touching a real
/// terminal. `Rc<RefCell<_>>`-backed so a test can hold a handle to inspect
/// it after moving the sink into `AppState.clipboard` as a `Box<dyn
/// Clipboard>`.
#[derive(Debug, Default, Clone)]
pub struct TestSink(Rc<RefCell<Option<String>>>);

impl Clipboard for TestSink {
    fn set(&mut self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_string());
    }
}

impl TestSink {
    pub fn last(&self) -> Option<String> {
        self.0.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_matches_known_vectors() {
        // RFC 4648 test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_sink_records_the_last_set_call() {
        let mut sink = TestSink::default();
        assert_eq!(sink.last(), None);
        sink.set("first");
        assert_eq!(sink.last(), Some("first".to_string()));
        sink.set("second");
        assert_eq!(sink.last(), Some("second".to_string()));
    }

    #[test]
    fn test_sink_clone_shares_the_same_backing_cell() {
        let sink = TestSink::default();
        let mut handle = sink.clone();
        handle.set("via handle");
        assert_eq!(sink.last(), Some("via handle".to_string()));
    }
}
