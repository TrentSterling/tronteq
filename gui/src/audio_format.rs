//! Validated WASAPI mix formats and byte decoding, independent of COM and devices.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Float32,
    Pcm { container: u16, valid: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub rate: u32,
    pub channels: usize,
    pub stride: usize,
    pub encoding: Encoding,
}

impl Format {
    /// WAVEFORMATEX including cbSize bytes. Extensible samples are left aligned.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 18 {
            return Err("Incomplete audio format".into());
        }
        let word = |i| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let tag = word(0);
        let channels = word(2) as usize;
        let rate = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let stride = word(12) as usize;
        let bits = word(14);
        let extra = word(16) as usize;
        if bytes.len() < 18 + extra {
            return Err("Incomplete extended audio format".into());
        }
        if !(1..=64).contains(&channels) || !(8_000..=384_000).contains(&rate) {
            return Err(format!(
                "Unsupported audio layout: {channels} channels at {rate} Hz"
            ));
        }
        let (kind, valid) = if tag == 0xfffe {
            if extra < 22 {
                return Err("Incomplete WAVEFORMATEXTENSIBLE".into());
            }
            // Standard PCM/IEEE_FLOAT GUID tail, encoded in WAVE's little endian layout.
            let tail = [0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71];
            if bytes[28..40] != tail {
                return Err("Unsupported audio subformat".into());
            }
            let sub = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
            (sub, word(18))
        } else {
            (tag as u32, bits)
        };
        let encoding = match (kind, bits) {
            (3, 32) if valid == 32 => Encoding::Float32,
            (1, 8 | 16 | 24 | 32) if valid > 0 && valid <= bits => Encoding::Pcm {
                container: bits,
                valid,
            },
            _ => {
                return Err(format!(
                    "Unsupported audio encoding: tag {kind}, {valid}/{bits} bits"
                ))
            }
        };
        if stride != channels * (bits as usize / 8) {
            return Err("Audio block alignment does not match its channels and sample size".into());
        }
        Ok(Self {
            rate,
            channels,
            stride,
            encoding,
        })
    }

    pub fn label(self) -> String {
        match self.encoding {
            Encoding::Float32 => "32-bit float".into(),
            Encoding::Pcm { container, valid } if container == valid => format!("{valid}-bit PCM"),
            Encoding::Pcm { container, valid } => format!("{valid}-bit PCM in {container} bits"),
        }
    }

    /// Decode one validated frame. Invalid plugin samples cannot poison analysis.
    pub fn decode(self, frame: &[u8], output: &mut [f32]) {
        assert!(frame.len() >= self.stride && output.len() >= self.channels);
        let size = self.stride / self.channels;
        for (bytes, sample) in frame[..self.stride]
            .chunks_exact(size)
            .zip(output.iter_mut())
        {
            *sample = match self.encoding {
                Encoding::Float32 => {
                    let v = f32::from_le_bytes(bytes.try_into().unwrap());
                    if v.is_finite() {
                        v.clamp(-16.0, 16.0)
                    } else {
                        0.0
                    }
                }
                Encoding::Pcm { container, valid } => {
                    let integer = match container {
                        8 => bytes[0] as i32 - 128,
                        16 => i16::from_le_bytes(bytes.try_into().unwrap()) as i32,
                        24 => i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8,
                        32 => i32::from_le_bytes(bytes.try_into().unwrap()),
                        _ => unreachable!(),
                    };
                    (integer >> (container - valid)) as f32 / (1u64 << (valid - 1)) as f32
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(tag: u16, bits: u16, valid: u16, subtype: u32) -> Vec<u8> {
        let mut b = vec![0; if tag == 0xfffe { 40 } else { 18 }];
        for (i, v) in [
            (0, tag),
            (2, 2),
            (12, bits / 4),
            (14, bits),
            (16, if tag == 0xfffe { 22 } else { 0 }),
        ] {
            b[i..i + 2].copy_from_slice(&v.to_le_bytes());
        }
        b[4..8].copy_from_slice(&48_000u32.to_le_bytes());
        if tag == 0xfffe {
            b[18..20].copy_from_slice(&valid.to_le_bytes());
            b[24..28].copy_from_slice(&subtype.to_le_bytes());
            b[28..40].copy_from_slice(&[0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
        }
        b
    }
    #[test]
    fn extensible_pcm_is_not_float_and_valid_bits_are_left_aligned() {
        let f = Format::parse(&descriptor(0xfffe, 32, 24, 1)).unwrap();
        let mut out = [0.0; 2];
        let bytes: Vec<_> = [0x40000000i32, -0x40000000]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        f.decode(&bytes, &mut out);
        assert_eq!(out, [0.5, -0.5]);
        assert_eq!(f.label(), "24-bit PCM in 32 bits");
        assert_eq!(
            Format::parse(&descriptor(0xfffe, 32, 32, 3))
                .unwrap()
                .encoding,
            Encoding::Float32
        );
    }
    #[test]
    fn pcm_formats_and_float_glitches_decode_without_cancellation_or_nan() {
        for (bits, bytes) in [
            (8, vec![192, 64]),
            (16, vec![0, 64, 0, 192]),
            (24, vec![0, 0, 64, 0, 0, 192]),
            (32, vec![0, 0, 0, 64, 0, 0, 0, 192]),
        ] {
            let f = Format::parse(&descriptor(1, bits, bits, 1)).unwrap();
            let mut out = [0.0; 2];
            f.decode(&bytes, &mut out);
            assert_eq!(out, [0.5, -0.5], "{bits}-bit PCM");
        }
        let f = Format::parse(&descriptor(3, 32, 32, 3)).unwrap();
        let mut out = [1.0; 2];
        let bytes: Vec<_> = [f32::NAN, f32::INFINITY]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        f.decode(&bytes, &mut out);
        assert_eq!(out, [0.0; 2]);
    }
    #[test]
    fn malformed_and_unknown_formats_are_errors() {
        assert!(Format::parse(&[0; 17]).is_err());
        assert!(Format::parse(&descriptor(0xfffe, 32, 32, 99)).is_err());
        assert!(Format::parse(&descriptor(0xfffe, 32, 33, 1)).is_err());
        let mut b = descriptor(0xfffe, 32, 32, 3);
        b[28] = 1;
        assert!(Format::parse(&b).is_err());
        b = descriptor(3, 32, 32, 3);
        b[12] = 1;
        assert!(Format::parse(&b).is_err());
        b = descriptor(3, 32, 32, 3);
        b[2] = 0;
        assert!(Format::parse(&b).is_err());
        b = descriptor(3, 32, 32, 3);
        b[4..8].fill(0);
        assert!(Format::parse(&b).is_err());
    }
}
