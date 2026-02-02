//! NAL unit parsing and AVCC conversion utilities.
//!
//! H.264 video can be packaged in two formats:
//! - **Annex B**: Uses start codes (0x000001 or 0x00000001) to separate NAL units.
//!   This is what x264 outputs.
//! - **AVCC**: Uses length prefixes (typically 4 bytes) before each NAL unit.
//!   This is what RTMP/FLV expects.
//!
//! This module provides utilities to convert between these formats and to build
//! the AVC Decoder Configuration Record (sequence header) required by RTMP.

use bytes::{BufMut, Bytes, BytesMut};
use tracing::debug;

/// NAL unit types relevant for H.264.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NalUnitType {
    /// Non-IDR slice (P/B frame).
    NonIdrSlice = 1,
    /// IDR slice (keyframe).
    IdrSlice = 5,
    /// Supplemental Enhancement Information.
    Sei = 6,
    /// Sequence Parameter Set.
    Sps = 7,
    /// Picture Parameter Set.
    Pps = 8,
    /// Access Unit Delimiter.
    Aud = 9,
    /// Other/unknown NAL type.
    Other = 0,
}

impl From<u8> for NalUnitType {
    fn from(byte: u8) -> Self {
        match byte & 0x1F {
            1 => NalUnitType::NonIdrSlice,
            5 => NalUnitType::IdrSlice,
            6 => NalUnitType::Sei,
            7 => NalUnitType::Sps,
            8 => NalUnitType::Pps,
            9 => NalUnitType::Aud,
            _ => NalUnitType::Other,
        }
    }
}

/// A single NAL unit extracted from an Annex B stream.
#[derive(Debug, Clone)]
pub struct NalUnit {
    /// The NAL unit type.
    pub nal_type: NalUnitType,
    /// The NAL unit data (including the NAL header byte, excluding start code).
    pub data: Bytes,
}

/// Parse an Annex B byte stream into individual NAL units.
///
/// Annex B format uses start codes (0x000001 or 0x00000001) to separate NAL units.
/// This function finds all NAL units and returns them without the start codes.
pub fn parse_annex_b(data: &[u8]) -> Vec<NalUnit> {
    let mut nals = Vec::new();
    let mut i = 0;
    let len = data.len();

    while i < len {
        // Find start code (0x000001 or 0x00000001)
        let start_code_len = if i + 3 < len && data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                3 // 0x000001
            } else if i + 4 <= len && data[i + 2] == 0 && data[i + 3] == 1 {
                4 // 0x00000001
            } else {
                i += 1;
                continue;
            }
        } else {
            i += 1;
            continue;
        };

        let nal_start = i + start_code_len;

        // Find next start code or end of data
        let mut nal_end = len;
        let mut j = nal_start;
        while j + 2 < len {
            if data[j] == 0
                && data[j + 1] == 0
                && (data[j + 2] == 1 || (j + 3 < len && data[j + 2] == 0 && data[j + 3] == 1))
            {
                nal_end = j;
                break;
            }
            j += 1;
        }

        if nal_start < nal_end {
            let nal_data = &data[nal_start..nal_end];
            if !nal_data.is_empty() {
                let nal_type = NalUnitType::from(nal_data[0]);
                nals.push(NalUnit {
                    nal_type,
                    data: Bytes::copy_from_slice(nal_data),
                });
            }
        }

        i = nal_end;
    }

    nals
}

/// Convert NAL units to AVCC format with 4-byte length prefixes.
///
/// AVCC format prepends each NAL unit with its length (big-endian).
/// The length prefix size is typically 4 bytes for H.264 over RTMP.
pub fn nals_to_avcc(nals: &[NalUnit]) -> Bytes {
    let mut buf = BytesMut::new();

    for nal in nals {
        // 4-byte length prefix (big-endian)
        buf.put_u32(nal.data.len() as u32);
        buf.put_slice(&nal.data);
    }

    buf.freeze()
}

/// Extract SPS and PPS NAL units from Annex B header data.
///
/// The header data from x264 contains SPS and PPS NAL units that describe
/// the video stream parameters. These are needed to build the AVC decoder
/// configuration record.
pub fn extract_sps_pps(annex_b_headers: &[u8]) -> Option<(Bytes, Bytes)> {
    let nals = parse_annex_b(annex_b_headers);

    let mut sps: Option<Bytes> = None;
    let mut pps: Option<Bytes> = None;

    for nal in nals {
        match nal.nal_type {
            NalUnitType::Sps => {
                debug!(len = nal.data.len(), "Found SPS NAL unit");
                sps = Some(nal.data);
            }
            NalUnitType::Pps => {
                debug!(len = nal.data.len(), "Found PPS NAL unit");
                pps = Some(nal.data);
            }
            _ => {}
        }
    }

    match (sps, pps) {
        (Some(s), Some(p)) => Some((s, p)),
        _ => None,
    }
}

/// Build an AVC Decoder Configuration Record from SPS and PPS.
///
/// This is the "sequence header" that must be sent before any video frames
/// in RTMP/FLV. It tells the decoder how to interpret the H.264 stream.
///
/// Format (ISO 14496-15):
/// - configurationVersion: 1 byte (always 0x01)
/// - AVCProfileIndication: 1 byte (from SPS byte 1)
/// - profile_compatibility: 1 byte (from SPS byte 2)
/// - AVCLevelIndication: 1 byte (from SPS byte 3)
/// - lengthSizeMinusOne: 1 byte (0xFF = 4-byte NAL length)
/// - numOfSequenceParameterSets: 1 byte (0xE1 = 1 SPS, upper 3 bits reserved)
/// - sequenceParameterSetLength: 2 bytes (big-endian)
/// - sequenceParameterSetNALUnit: variable
/// - numOfPictureParameterSets: 1 byte
/// - pictureParameterSetLength: 2 bytes (big-endian)
/// - pictureParameterSetNALUnit: variable
pub fn build_avc_decoder_config(sps: &[u8], pps: &[u8]) -> Option<Bytes> {
    if sps.len() < 4 {
        debug!("SPS too short: {} bytes", sps.len());
        return None;
    }

    let mut buf = BytesMut::with_capacity(11 + sps.len() + pps.len());

    // configurationVersion
    buf.put_u8(0x01);

    // AVCProfileIndication, profile_compatibility, AVCLevelIndication
    buf.put_u8(sps[1]); // profile_idc
    buf.put_u8(sps[2]); // constraint flags
    buf.put_u8(sps[3]); // level_idc

    // lengthSizeMinusOne (0xFF = 4-byte NAL length prefix, upper 6 bits reserved as 1s)
    buf.put_u8(0xFF);

    // numOfSequenceParameterSets (0xE1 = 1 SPS, upper 3 bits reserved as 1s)
    buf.put_u8(0xE1);

    // SPS length and data
    buf.put_u16(sps.len() as u16);
    buf.put_slice(sps);

    // numOfPictureParameterSets
    buf.put_u8(0x01);

    // PPS length and data
    buf.put_u16(pps.len() as u16);
    buf.put_slice(pps);

    debug!(
        sps_len = sps.len(),
        pps_len = pps.len(),
        total_len = buf.len(),
        "Built AVC decoder configuration record"
    );

    Some(buf.freeze())
}

/// Build an FLV video tag payload for H.264 data.
///
/// FLV video tag format:
/// - Frame Type (4 bits) + Codec ID (4 bits): 1 byte
///   - Frame Type: 1=keyframe, 2=inter frame
///   - Codec ID: 7=AVC (H.264)
/// - AVC Packet Type: 1 byte
///   - 0=AVC sequence header (decoder config)
///   - 1=AVC NALU (video data)
/// - Composition Time Offset: 3 bytes (signed, big-endian)
///   - For sequence header: 0
///   - For video: PTS - DTS
/// - Data: variable
pub fn build_flv_video_tag(
    data: &[u8],
    is_keyframe: bool,
    is_sequence_header: bool,
    composition_time: i32,
) -> Bytes {
    let mut buf = BytesMut::with_capacity(5 + data.len());

    // Frame Type (4 bits) + Codec ID (4 bits)
    // Frame Type: 1=keyframe, 2=inter frame
    // Codec ID: 7=AVC
    let frame_type = if is_keyframe { 0x10 } else { 0x20 };
    let codec_id = 0x07; // AVC
    buf.put_u8(frame_type | codec_id);

    // AVC Packet Type: 0=sequence header, 1=NALU
    buf.put_u8(if is_sequence_header { 0x00 } else { 0x01 });

    // Composition Time Offset (3 bytes, signed big-endian)
    // For live streaming, this is typically 0
    let ct = composition_time as u32;
    buf.put_u8(((ct >> 16) & 0xFF) as u8);
    buf.put_u8(((ct >> 8) & 0xFF) as u8);
    buf.put_u8((ct & 0xFF) as u8);

    // Data
    buf.put_slice(data);

    buf.freeze()
}

/// Filter NAL units, removing SPS/PPS (which should be in the sequence header).
///
/// When sending video frames, we don't want to include SPS/PPS NAL units
/// because they're already in the sequence header sent at stream start.
pub fn filter_parameter_sets(nals: Vec<NalUnit>) -> Vec<NalUnit> {
    nals.into_iter()
        .filter(|nal| {
            !matches!(
                nal.nal_type,
                NalUnitType::Sps | NalUnitType::Pps | NalUnitType::Aud
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_annex_b_3byte_start_code() {
        // NAL with 3-byte start code
        let data = [0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1E]; // SPS
        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_type, NalUnitType::Sps);
        assert_eq!(nals[0].data.as_ref(), &[0x67, 0x42, 0x00, 0x1E]);
    }

    #[test]
    fn test_parse_annex_b_4byte_start_code() {
        // NAL with 4-byte start code
        let data = [0x00, 0x00, 0x00, 0x01, 0x68, 0xCE, 0x3C, 0x80]; // PPS
        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_type, NalUnitType::Pps);
    }

    #[test]
    fn test_parse_annex_b_multiple_nals() {
        // SPS followed by PPS
        let data = [
            0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1E, // SPS
            0x00, 0x00, 0x00, 0x01, 0x68, 0xCE, 0x3C, 0x80, // PPS
        ];
        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 2);
        assert_eq!(nals[0].nal_type, NalUnitType::Sps);
        assert_eq!(nals[1].nal_type, NalUnitType::Pps);
    }

    #[test]
    fn test_nals_to_avcc() {
        let nals = vec![NalUnit {
            nal_type: NalUnitType::IdrSlice,
            data: Bytes::from_static(&[0x65, 0x88, 0x84]),
        }];
        let avcc = nals_to_avcc(&nals);
        // 4-byte length (3) + 3 bytes data
        assert_eq!(avcc.as_ref(), &[0x00, 0x00, 0x00, 0x03, 0x65, 0x88, 0x84]);
    }

    #[test]
    fn test_build_avc_decoder_config() {
        let sps = [0x67, 0x42, 0x00, 0x1E, 0xAB, 0xCD]; // Fake SPS
        let pps = [0x68, 0xCE, 0x3C, 0x80]; // Fake PPS

        let config = build_avc_decoder_config(&sps, &pps).unwrap();

        // Check header
        assert_eq!(config[0], 0x01); // configurationVersion
        assert_eq!(config[1], 0x42); // profile_idc (from SPS[1])
        assert_eq!(config[2], 0x00); // constraint flags (from SPS[2])
        assert_eq!(config[3], 0x1E); // level_idc (from SPS[3])
        assert_eq!(config[4], 0xFF); // lengthSizeMinusOne
        assert_eq!(config[5], 0xE1); // numOfSequenceParameterSets
    }

    #[test]
    fn test_build_flv_video_tag_keyframe() {
        let data = [0x65, 0x88, 0x84];
        let tag = build_flv_video_tag(&data, true, false, 0);

        assert_eq!(tag[0], 0x17); // keyframe (0x10) + AVC (0x07)
        assert_eq!(tag[1], 0x01); // AVC NALU
        assert_eq!(tag[2], 0x00); // composition time
        assert_eq!(tag[3], 0x00);
        assert_eq!(tag[4], 0x00);
        assert_eq!(&tag[5..], &data);
    }

    #[test]
    fn test_build_flv_video_tag_sequence_header() {
        let data = [0x01, 0x42, 0x00, 0x1E];
        let tag = build_flv_video_tag(&data, true, true, 0);

        assert_eq!(tag[0], 0x17); // keyframe (0x10) + AVC (0x07)
        assert_eq!(tag[1], 0x00); // AVC sequence header
    }

    #[test]
    fn test_filter_parameter_sets() {
        let nals = vec![
            NalUnit {
                nal_type: NalUnitType::Sps,
                data: Bytes::from_static(&[0x67]),
            },
            NalUnit {
                nal_type: NalUnitType::Pps,
                data: Bytes::from_static(&[0x68]),
            },
            NalUnit {
                nal_type: NalUnitType::IdrSlice,
                data: Bytes::from_static(&[0x65]),
            },
        ];

        let filtered = filter_parameter_sets(nals);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].nal_type, NalUnitType::IdrSlice);
    }
}
