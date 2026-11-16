//! H.264 in its two wire forms. Encoders hand over Annex B (start codes);
//! FLV and MP4 want AVCC (each unit prefixed with its length).

/// NAL unit types the streamer cares about.
const NAL_SPS: u8 = 7;
const NAL_PPS: u8 = 8;
const NAL_AUD: u8 = 9;

/// The NAL units of an Annex B buffer, without their start codes.
pub fn nal_units(data: &[u8]) -> Vec<&[u8]> {
    // Positions just after each start code (00 00 01), and where it began.
    let mut starts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push((i, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut units = Vec::new();
    for (n, &(_, begin)) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(data.len(), |&(s, _)| s);
        // A four-byte start code leaves a zero at the end of the previous unit.
        while end > begin && data[end - 1] == 0 {
            end -= 1;
        }
        if end > begin {
            units.push(&data[begin..end]);
        }
    }
    units
}

/// The type of a NAL unit.
pub fn nal_type(unit: &[u8]) -> u8 {
    unit.first().map_or(0, |b| b & 0x1F)
}

/// The first SPS and PPS of a buffer, when it has both.
pub fn parameter_sets(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let units = nal_units(data);
    let sps = units.iter().find(|u| nal_type(u) == NAL_SPS)?;
    let pps = units.iter().find(|u| nal_type(u) == NAL_PPS)?;
    Some((sps.to_vec(), pps.to_vec()))
}

/// The picture units as AVCC: each prefixed by its 4-byte length. The
/// parameter sets and access-unit delimiters travel in the sequence header, so
/// they are left out here.
pub fn to_avcc(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    for unit in nal_units(data) {
        if matches!(nal_type(unit), NAL_SPS | NAL_PPS | NAL_AUD) {
            continue;
        }
        out.extend_from_slice(&(unit.len() as u32).to_be_bytes());
        out.extend_from_slice(unit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut d = vec![0, 0, 0, 1, 0x09, 0xF0]; // AUD
        d.extend([0, 0, 0, 1, 0x67, 0x64, 0x00, 0x28]); // SPS
        d.extend([0, 0, 1, 0x68, 0xEE, 0x3C]); // PPS, short start code
        d.extend([0, 0, 0, 1, 0x65, 0x88, 0x84]); // IDR slice
        d
    }

    #[test]
    fn splits_on_three_and_four_byte_start_codes() {
        let d = sample();
        let units = nal_units(&d);
        let types: Vec<u8> = units.iter().map(|u| nal_type(u)).collect();
        assert_eq!(types, [9, 7, 8, 5]);
        assert_eq!(units[1], [0x67, 0x64, 0x00, 0x28]);
        assert_eq!(units[2], [0x68, 0xEE, 0x3C]);
    }

    #[test]
    fn finds_the_parameter_sets() {
        let (sps, pps) = parameter_sets(&sample()).unwrap();
        assert_eq!(sps, [0x67, 0x64, 0x00, 0x28]);
        assert_eq!(pps, [0x68, 0xEE, 0x3C]);
        assert!(parameter_sets(&[0, 0, 1, 0x65, 1]).is_none());
    }

    #[test]
    fn avcc_keeps_only_the_picture_with_lengths() {
        assert_eq!(to_avcc(&sample()), [0, 0, 0, 3, 0x65, 0x88, 0x84]);
    }

    #[test]
    fn nothing_without_a_start_code() {
        assert!(nal_units(&[1, 2, 3]).is_empty());
        assert!(to_avcc(&[]).is_empty());
    }
}
