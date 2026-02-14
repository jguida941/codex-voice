//! ANSI/control stripping so prompt matching runs on stable plain-text buffers.

use vte::{Parser as VteParser, Perform};

pub(super) fn strip_ansi_preserve_controls(bytes: &[u8]) -> Vec<u8> {
    struct ControlStripper {
        output: Vec<u8>,
    }

    impl Perform for ControlStripper {
        fn print(&mut self, c: char) {
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            self.output.extend_from_slice(encoded.as_bytes());
        }

        fn execute(&mut self, byte: u8) {
            match byte {
                b'\n' | b'\r' | b'\t' => self.output.push(byte),
                _ => {}
            }
        }
    }

    let mut parser = VteParser::new();
    let mut stripper = ControlStripper {
        output: Vec::with_capacity(bytes.len()),
    };
    parser.advance(&mut stripper, bytes);
    stripper.output
}
