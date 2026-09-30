pub(crate) struct TagPath {
    pub bytes: Vec<u8>,
    pub type_code: Option<u16>,
}

pub(crate) fn encode(address: &str) -> Result<TagPath, String> {
    let (tag, type_code) = if let Some(rest) = address.strip_prefix("type=") {
        let (kind, tag) = rest
            .split_once(';')
            .ok_or("CIP type override requires type=...;Tag")?;
        let kind = if let Some(hex) = kind.strip_prefix("0x").or_else(|| kind.strip_prefix("0X")) {
            u16::from_str_radix(hex, 16)
        } else {
            kind.parse()
        }
        .map_err(|_| "Invalid CIP type override")?;
        (tag, Some(kind))
    } else {
        (address, None)
    };
    if tag.is_empty() || tag.len() > 1024 || !tag.is_ascii() {
        return Err("CIP requires a nonempty ASCII symbolic tag".into());
    }
    let input = tag.as_bytes();
    let mut offset = 0;
    let mut path = Vec::with_capacity((tag.len() + 8).min(510));
    loop {
        let start = offset;
        if !input
            .get(offset)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        {
            return Err("Invalid CIP symbolic tag segment".into());
        }
        offset += 1;
        while input
            .get(offset)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b':'))
        {
            offset += 1;
        }
        let symbol = &input[start..offset];
        if symbol.len() > 255 {
            return Err("CIP symbol exceeds 255 bytes".into());
        }
        path.extend_from_slice(&[0x91, symbol.len() as u8]);
        path.extend_from_slice(symbol);
        if symbol.len() % 2 != 0 {
            path.push(0);
        }
        while input.get(offset) == Some(&b'[') {
            offset += 1;
            loop {
                let start = offset;
                while input.get(offset).is_some_and(u8::is_ascii_digit) {
                    offset += 1;
                }
                let index: u32 = tag[start..offset]
                    .parse()
                    .map_err(|_| "Invalid CIP array index")?;
                if index <= u8::MAX as u32 {
                    path.extend_from_slice(&[0x28, index as u8]);
                } else if index <= u16::MAX as u32 {
                    path.extend_from_slice(&[0x29, 0]);
                    path.extend_from_slice(&(index as u16).to_le_bytes());
                } else {
                    path.extend_from_slice(&[0x2a, 0]);
                    path.extend_from_slice(&index.to_le_bytes());
                }
                match input.get(offset) {
                    Some(b',') => offset += 1,
                    Some(b']') => {
                        offset += 1;
                        break;
                    }
                    _ => return Err("Unterminated CIP array index".into()),
                }
                if path.len() > 510 {
                    return Err("CIP path exceeds 255 words".into());
                }
            }
        }
        if path.len() > 510 {
            return Err("CIP path exceeds 255 words".into());
        }
        if offset == input.len() {
            break;
        }
        if input[offset] != b'.' {
            return Err("Unsupported CIP address syntax".into());
        }
        offset += 1;
    }
    Ok(TagPath {
        bytes: path,
        type_code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_and_array_segments_match_cip_epath() {
        assert_eq!(
            encode("Tag[1,256].X[65536]").unwrap().bytes,
            [
                0x91, 3, b'T', b'a', b'g', 0, 0x28, 1, 0x29, 0, 0, 1, 0x91, 1, b'X', 0, 0x2a, 0, 0,
                0, 1, 0
            ]
        );
        assert_eq!(encode("type=0xD2;A").unwrap().type_code, Some(0xd2));
        assert!(encode("Program:Main.Tag").is_ok());
    }

    #[test]
    fn invalid_addresses_are_rejected() {
        for address in [
            "",
            ".A",
            "A.",
            "A[]",
            "A[-1]",
            "A[1,]",
            "A[1",
            "A[4294967296]",
            "A\0B",
            "A B",
            "slot=1;A",
            "type=bad;A",
            "标签",
        ] {
            assert!(encode(address).is_err(), "{address}");
        }
        assert!(encode(&"A".repeat(256)).is_err());
    }
}
