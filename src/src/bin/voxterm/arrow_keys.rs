//! Arrow-key normalization so directional input maps consistently across terminals.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

#[inline]
fn map_arrow_final(byte: u8) -> Option<ArrowKey> {
    match byte {
        b'A' => Some(ArrowKey::Up),
        b'B' => Some(ArrowKey::Down),
        b'C' => Some(ArrowKey::Right),
        b'D' => Some(ArrowKey::Left),
        _ => None,
    }
}

#[inline]
fn is_csi_final(byte: u8) -> bool {
    (0x40..=0x7e).contains(&byte)
}

fn parse_arrow_sequence(bytes: &[u8], start: usize) -> Option<(ArrowKey, usize)> {
    if start.checked_add(1).is_none_or(|idx| idx >= bytes.len()) || bytes[start] != 0x1b {
        return None;
    }
    match bytes[start + 1] {
        b'O' => {
            let idx = start.checked_add(2)?;
            let key = map_arrow_final(*bytes.get(idx)?)?;
            Some((key, idx + 1))
        }
        b'[' => {
            let mut idx = start.checked_add(2)?;
            while idx < bytes.len() {
                let byte = bytes[idx];
                if let Some(key) = map_arrow_final(byte) {
                    return Some((key, idx + 1));
                }
                if is_csi_final(byte) {
                    return None;
                }
                if byte.is_ascii_digit() || byte == b';' {
                    idx += 1;
                    continue;
                }
                return None;
            }
            None
        }
        _ => None,
    }
}

pub(crate) fn parse_arrow_keys(bytes: &[u8]) -> Vec<ArrowKey> {
    let mut keys = Vec::new();
    let mut idx: usize = 0;
    while idx < bytes.len() {
        if let Some((key, next_idx)) = parse_arrow_sequence(bytes, idx) {
            keys.push(key);
            idx = next_idx;
        } else {
            idx += 1;
        }
    }
    keys
}

pub(crate) fn parse_arrow_keys_only(bytes: &[u8]) -> Option<Vec<ArrowKey>> {
    if bytes.is_empty() {
        return None;
    }
    let mut keys = Vec::new();
    let mut idx: usize = 0;
    while idx < bytes.len() {
        let (key, next_idx) = parse_arrow_sequence(bytes, idx)?;
        keys.push(key);
        idx = next_idx;
    }
    Some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_arrow_keys_reads_sequences() {
        let bytes = [
            0x1b, b'[', b'A', 0x1b, b'[', b'B', b'x', 0x1b, b'O', b'C', 0x1b, b'[', b'D',
        ];
        let keys = parse_arrow_keys(&bytes);
        assert_eq!(
            keys,
            vec![
                ArrowKey::Up,
                ArrowKey::Down,
                ArrowKey::Right,
                ArrowKey::Left
            ]
        );
        assert!(parse_arrow_keys(&[0x1b, b'[']).is_empty());
    }

    #[test]
    fn parse_arrow_keys_supports_parameterized_csi_sequences() {
        let bytes = [
            0x1b, b'[', b'1', b';', b'2', b'A', 0x1b, b'[', b'1', b';', b'5', b'D',
        ];
        let keys = parse_arrow_keys(&bytes);
        assert_eq!(keys, vec![ArrowKey::Up, ArrowKey::Left]);
    }

    #[test]
    fn parse_arrow_keys_only_accepts_parameterized_sequences() {
        let bytes = [
            0x1b, b'[', b'1', b';', b'2', b'C', 0x1b, b'[', b'1', b';', b'3', b'D',
        ];
        let keys = parse_arrow_keys_only(&bytes).expect("parameterized arrows");
        assert_eq!(keys, vec![ArrowKey::Right, ArrowKey::Left]);
    }

    #[test]
    fn parse_arrow_keys_only_rejects_non_arrow_csi_sequences() {
        assert!(parse_arrow_keys_only(&[0x1b, b'[', b'1', b';', b'2', b'P']).is_none());
        assert!(parse_arrow_keys_only(b"abc").is_none());
    }
}
