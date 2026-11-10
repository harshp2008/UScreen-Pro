//! Annex B packetizer for the CLI encoder path: splits ffmpeg's byte stream into access units,
//! tags keyframes, and extracts SPS/PPS(/VPS) so a client that joins mid-stream can configure its
//! decoder. Understands both H.264 (one-byte NAL header) and HEVC (two-byte header, different
//! type numbers).
//!
//! Only compiled without the `inproc-encoder` feature.

use super::{Codec, VideoPacket};
use bytes::Bytes;

// NAL unit types. Only the CLI path parses the bitstream itself; with the
// in-process encoder libavcodec hands back one complete access unit per frame.
//
// HEVC types are from H.265 Table 7-1. IRAP covers every type a decoder may
// start from, not only IDR: a stream may open on a CRA.
/// Highest HEVC NAL type that carries picture data (VCL); types 32 and up are parameter sets and SEI.
const HEVC_NAL_VCL_MAX: u8 = 31;
/// First HEVC IRAP type (BLA_W_LP): a picture a decoder may start from.
const HEVC_NAL_IRAP_MIN: u8 = 16;
/// Last HEVC IRAP type; 22 and 23 are reserved but kept in range so a future stream is still treated as a join point.
const HEVC_NAL_IRAP_MAX: u8 = 23;
/// HEVC video parameter set. Part of the codec config a joining client needs.
const HEVC_NAL_VPS: u8 = 32;
/// HEVC sequence parameter set (carries the picture size).
const HEVC_NAL_SPS: u8 = 33;
/// HEVC picture parameter set.
const HEVC_NAL_PPS: u8 = 34;
/// HEVC access unit delimiter: marks where one picture ends and the next begins.
const HEVC_NAL_AUD: u8 = 35;

/// What a NAL unit means to the packetizer, after the codec-specific type number is decoded.
/// SPS, PPS and the delimiter are boundaries or config; VCL carries picture data; everything else (SEI, filler) is passed through.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NalKind {
    Sps,
    Pps,
    Aud,
    Vcl,
    Other,
}

/// H.264 `nal_unit_type` of a non-IDR slice: an ordinary inter-coded picture.
const NAL_TYPE_NON_IDR: u8 = 1;
/// H.264 `nal_unit_type` of an IDR slice: a keyframe a client can join on.
const NAL_TYPE_IDR: u8 = 5;
/// H.264 `nal_unit_type` of an access unit delimiter.
const NAL_TYPE_AUD: u8 = 9;
/// H.264 `nal_unit_type` of the sequence parameter set.
const NAL_TYPE_SPS: u8 = 7;
/// H.264 `nal_unit_type` of the picture parameter set.
const NAL_TYPE_PPS: u8 = 8;

/// Splits an Annex B byte stream (H.264 or HEVC) into one `VideoPacket` per access unit.
/// Holds partial NALs between reads; the last NAL is only released once the next start code, or `finish`, proves it complete.
pub(super) struct H264AnnexBPacketizer {
    buffer: Vec<u8>,
    pending_access_unit: Vec<u8>,
    pending_has_vcl: bool,
    pending_has_idr: bool,
    config: Vec<u8>,
    has_sps: bool,
    has_pps: bool,
    next_seq: u32,
    codec: Codec,
}

impl H264AnnexBPacketizer {
    /// A packetizer for `codec`'s byte stream, with nothing buffered yet.
    pub(super) fn new(codec: Codec) -> Self {
        Self {
            buffer: Vec::new(),
            pending_access_unit: Vec::new(),
            pending_has_vcl: false,
            pending_has_idr: false,
            config: Vec::new(),
            has_sps: false,
            has_pps: false,
            next_seq: 0,
            codec,
        }
    }

    /// Feed encoder output; returns the complete access units found so far.
    pub(super) fn push(&mut self, data: &[u8]) -> Vec<VideoPacket> {
        self.buffer.extend_from_slice(data);
        self.process_complete_nals(false)
    }

    /// Take one whole encoded frame and release it without waiting for the
    /// next one.
    pub(super) fn push_frame(&mut self, frame: &[u8]) -> Vec<VideoPacket> {
        let mut out = self.push(frame);
        out.extend(self.finish());
        out
    }

    /// Flush at the end of the stream: whatever is buffered becomes the last packets.
    pub(super) fn finish(&mut self) -> Vec<VideoPacket> {
        let mut out = self.process_complete_nals(true);
        if let Some(access_unit) = self.take_pending_access_unit() {
            out.push(access_unit);
        }
        out
    }

    /// The parameter sets (SPS/PPS, plus VPS for HEVC) a new client needs first; `None` until all
    /// have been seen.
    pub(super) fn codec_config(&self) -> Option<Bytes> {
        if self.has_sps && self.has_pps {
            Some(Bytes::copy_from_slice(&self.config))
        } else {
            None
        }
    }

    /// Split the buffer at start codes and handle every NAL that is complete (all of them when
    /// `flush`).
    fn process_complete_nals(&mut self, flush: bool) -> Vec<VideoPacket> {
        let mut out = Vec::new();
        let starts = find_start_codes(&self.buffer);

        if starts.is_empty() {
            if self.buffer.len() > 3 {
                let keep_from = self.buffer.len() - 3;
                self.buffer.drain(..keep_from);
            }
            return out;
        }

        if starts[0] > 0 {
            self.buffer.drain(..starts[0]);
        }

        let starts = find_start_codes(&self.buffer);
        if starts.len() < 2 && !flush {
            return out;
        }

        let nal_count = if flush {
            starts.len()
        } else {
            starts.len() - 1
        };
        for idx in 0..nal_count {
            let start = starts[idx];
            let end = starts.get(idx + 1).copied().unwrap_or(self.buffer.len());
            let nal = self.buffer[start..end].to_vec();
            self.process_nal(&nal, &mut out);
        }

        let drain_to = if flush {
            self.buffer.len()
        } else {
            starts[starts.len() - 1]
        };
        self.buffer.drain(..drain_to);
        out
    }

    /// Classify one NAL (parameter set, slice, delimiter) and add it to the access unit being
    /// built, emitting the previous unit when a new frame starts.
    fn process_nal(&mut self, nal: &[u8], out: &mut Vec<VideoPacket>) {
        let Some(header_offset) = nal_header_offset(nal, 0) else {
            return;
        };
        if header_offset >= nal.len() {
            return;
        }

        let (kind, is_key) = match self.codec {
            Codec::H264 => {
                let t = nal[header_offset] & 0x1f;
                let kind = match t {
                    NAL_TYPE_SPS => NalKind::Sps,
                    NAL_TYPE_PPS => NalKind::Pps,
                    NAL_TYPE_AUD => NalKind::Aud,
                    NAL_TYPE_NON_IDR..=NAL_TYPE_IDR => NalKind::Vcl,
                    _ => NalKind::Other,
                };
                (kind, t == NAL_TYPE_IDR)
            }
            Codec::Hevc => {
                // Two-byte header; the type is bits 1..6 of the first byte.
                let t = (nal[header_offset] >> 1) & 0x3f;
                let kind = match t {
                    HEVC_NAL_VPS | HEVC_NAL_SPS => NalKind::Sps,
                    HEVC_NAL_PPS => NalKind::Pps,
                    HEVC_NAL_AUD => NalKind::Aud,
                    0..=HEVC_NAL_VCL_MAX => NalKind::Vcl,
                    _ => NalKind::Other,
                };
                // Any IRAP picture is a valid place for a decoder to join,
                // not only an IDR — refusing a CRA would leave a client
                // waiting for a picture the encoder may never emit.
                (kind, (HEVC_NAL_IRAP_MIN..=HEVC_NAL_IRAP_MAX).contains(&t))
            }
        };

        match kind {
            NalKind::Sps => {
                self.has_sps = true;
                self.config.extend_from_slice(nal);
            }
            NalKind::Pps => {
                self.has_pps = true;
                self.config.extend_from_slice(nal);
            }
            NalKind::Aud => {
                if let Some(access_unit) = self.take_pending_access_unit() {
                    out.push(access_unit);
                }
                self.pending_access_unit.extend_from_slice(nal);
            }
            NalKind::Vcl => {
                if self.pending_has_vcl && self.starts_new_picture(nal, header_offset) {
                    if let Some(access_unit) = self.take_pending_access_unit() {
                        out.push(access_unit);
                    }
                }
                if is_key {
                    self.pending_has_idr = true;
                }
                self.pending_access_unit.extend_from_slice(nal);
                self.pending_has_vcl = true;
            }
            NalKind::Other => {
                self.pending_access_unit.extend_from_slice(nal);
            }
        }
    }

    /// Is this slice the first of a new picture?
    ///
    /// H.264 answers with `first_mb_in_slice == 0`, which needs the
    /// exp-Golomb reader. HEVC puts `first_slice_segment_in_pic_flag` in the
    /// very first bit after its two-byte header, so it is just a bit test.
    fn starts_new_picture(&self, nal: &[u8], header_offset: usize) -> bool {
        match self.codec {
            Codec::H264 => Self::first_mb_in_slice(nal, header_offset) == Some(0),
            Codec::Hevc => nal.get(header_offset + 2).is_some_and(|b| b & 0x80 != 0),
        }
    }

    /// Take the access unit being built as a packet, or `None` if it holds no picture data.
    fn take_pending_access_unit(&mut self) -> Option<VideoPacket> {
        if !self.pending_has_vcl || self.pending_access_unit.is_empty() {
            self.pending_access_unit.clear();
            self.pending_has_vcl = false;
            self.pending_has_idr = false;
            return None;
        }

        let was_idr = self.pending_has_idr;
        self.pending_has_vcl = false;
        self.pending_has_idr = false;

        let au_data = std::mem::take(&mut self.pending_access_unit);

        // Prepend SPS/PPS to IDR frames so the decoder can always decode them,
        // even if it missed the initial config packet or reconnected mid-stream.
        let data = if was_idr && self.has_sps && self.has_pps {
            let mut full = Vec::with_capacity(self.config.len() + au_data.len());
            full.extend_from_slice(&self.config);
            full.extend_from_slice(&au_data);
            Bytes::from(full)
        } else {
            Bytes::from(au_data)
        };
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        Some(VideoPacket {
            data,
            is_idr: was_idr,
            seq,
        })
    }

    /// The `first_mb_in_slice` field of an H.264 slice header (0 means the slice starts a new
    /// frame).
    fn first_mb_in_slice(nal: &[u8], header_offset: usize) -> Option<u32> {
        let payload = nal.get(header_offset + 1..)?;
        ExpGolombReader::new(payload).read_ue()
    }
}

/// MSB-first bit reader with unsigned Exp-Golomb decoding; used for the H.264 `first_mb_in_slice` field.
/// HEVC's equivalent is a single flag bit and does not need it.
struct ExpGolombReader<'a> {
    data: &'a [u8],
    byte: usize,
    bit: u8,
}

impl<'a> ExpGolombReader<'a> {
    /// A bit reader over `data`.
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte: 0,
            bit: 0,
        }
    }

    /// Read one unsigned Exp-Golomb value, or `None` if it is malformed or runs past the data.
    fn read_ue(mut self) -> Option<u32> {
        let mut leading_zero_bits = 0u32;
        while self.read_bit()? == 0 {
            leading_zero_bits += 1;
            if leading_zero_bits > 31 {
                return None;
            }
        }

        let mut value = 1u32.checked_shl(leading_zero_bits)?;
        for shift in (0..leading_zero_bits).rev() {
            value |= (self.read_bit()? as u32) << shift;
        }
        Some(value - 1)
    }

    /// The next bit, most significant first.
    fn read_bit(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.byte)?;
        let value = (byte >> (7 - self.bit)) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.byte += 1;
        }
        Some(value)
    }
}

/// Find all NAL start codes in a buffer and return their positions.
pub(super) fn find_start_codes(data: &[u8]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i < data.len().saturating_sub(3) {
        // Check for 4-byte start code: 00 00 00 01
        if i + 3 < data.len()
            && data[i] == 0
            && data[i + 1] == 0
            && data[i + 2] == 0
            && data[i + 3] == 1
        {
            starts.push(i);
            i += 4;
        }
        // Check for 3-byte start code: 00 00 01
        else if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i);
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
}

/// Offset of the NAL header byte after the 3- or 4-byte start code at `start`, or `None`.
pub(super) fn nal_header_offset(data: &[u8], start: usize) -> Option<usize> {
    if start + 4 < data.len()
        && data[start] == 0
        && data[start + 1] == 0
        && data[start + 2] == 0
        && data[start + 3] == 1
    {
        Some(start + 4)
    } else if start + 3 < data.len()
        && data[start] == 0
        && data[start + 1] == 0
        && data[start + 2] == 1
    {
        Some(start + 3)
    } else {
        None
    }
}

/// Unit tests for access-unit splitting, keyframe tagging and SPS/PPS prepending.
#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a 4-byte-start-code H.264 NAL: header byte `nal_type` followed by `payload`.
    fn nal(nal_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut data = vec![0, 0, 0, 1, nal_type];
        data.extend_from_slice(payload);
        data
    }
    /// HEVC NAL: two-byte header, type in bits 1..6 of the first byte.
    fn hevc_nal(nal_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut data = vec![0, 0, 0, 1, nal_type << 1, 1];
        data.extend_from_slice(payload);
        data
    }
    /// A slice NAL whose first payload bit is `first_slice_segment_in_pic_flag`.
    fn hevc_slice(nal_type: u8, first_in_pic: bool) -> Vec<u8> {
        hevc_nal(nal_type, &[if first_in_pic { 0x80 } else { 0x00 }, 0x00])
    }
    /// VPS, SPS and PPS are collected into the codec config and the IRAP slice is tagged as a keyframe.
    #[test]
    fn hevc_parameter_sets_and_keyframes_are_recognised() {
        // A NAL is only parsed once the next start code proves it complete,
        // so anything pushed last stays buffered until finish().
        let mut incomplete = H264AnnexBPacketizer::new(Codec::Hevc);
        incomplete.push(&hevc_nal(HEVC_NAL_VPS, &[1, 2]));
        incomplete.push(&hevc_nal(HEVC_NAL_SPS, &[3, 4]));
        incomplete.finish();
        assert!(
            incomplete.codec_config().is_none(),
            "config is not complete without a PPS"
        );

        let mut p = H264AnnexBPacketizer::new(Codec::Hevc);
        let mut data = Vec::new();
        // VPS, SPS and PPS all belong to the decoder configuration.
        data.extend_from_slice(&hevc_nal(HEVC_NAL_VPS, &[1, 2]));
        data.extend_from_slice(&hevc_nal(HEVC_NAL_SPS, &[3, 4]));
        data.extend_from_slice(&hevc_nal(HEVC_NAL_PPS, &[5, 6]));
        data.extend_from_slice(&hevc_slice(19, true)); // IDR_W_RADL
        let mut out = p.push(&data);
        out.extend(p.finish());
        assert!(p.codec_config().is_some(), "VPS+SPS+PPS should complete it");
        assert_eq!(out.len(), 1);
        assert!(out[0].is_idr, "IDR_W_RADL must be marked as a keyframe");
    }
    /// A CRA picture is a legal join point and must be tagged as a keyframe, like an IDR.
    #[test]
    fn hevc_cra_counts_as_a_join_point() {
        // A decoder may start at any IRAP picture, not only an IDR. Treating
        // CRA as an ordinary frame would leave a joining client waiting for a
        // keyframe the encoder never sends.
        let mut p = H264AnnexBPacketizer::new(Codec::Hevc);
        p.push(&hevc_slice(21, true)); // CRA_NUT
        let out = p.finish();
        assert_eq!(out.len(), 1);
        assert!(out[0].is_idr, "CRA is a valid random access point");
    }
    /// Slices with `first_slice_segment_in_pic_flag` set start a new picture; the others continue the current one.
    #[test]
    fn hevc_splits_pictures_on_the_first_slice_flag() {
        let mut p = H264AnnexBPacketizer::new(Codec::Hevc);
        let mut data = Vec::new();
        data.extend_from_slice(&hevc_slice(1, true)); // picture 1 starts
        data.extend_from_slice(&hevc_slice(1, false)); // ...continues
        data.extend_from_slice(&hevc_slice(1, true)); // picture 2 starts
        let mut out = p.push(&data);
        out.extend(p.finish());
        assert_eq!(out.len(), 2, "two pictures, not three slices");
    }
    /// A complete frame is released immediately by `push_frame`, so latency does not include waiting for the next frame's start code.
    #[test]
    fn packetizer_releases_each_frame_without_waiting_for_the_next() {
        let mut packetizer = H264AnnexBPacketizer::new(Codec::H264);
        let idr = [
            nal(NAL_TYPE_SPS, &[1]),
            nal(NAL_TYPE_PPS, &[2]),
            nal(NAL_TYPE_IDR, &[0x80]),
        ]
        .concat();

        let first = packetizer.push_frame(&idr);
        assert_eq!(first.len(), 1);
        assert!(first[0].is_idr);

        let second = packetizer.push_frame(&nal(NAL_TYPE_NON_IDR, &[0x80]));
        assert_eq!(second.len(), 1);
        assert!(!second[0].is_idr);
        assert_eq!(second[0].seq, first[0].seq + 1);
    }

    /// A start code split across two reads (`00 00 | 00 01`) is still recognised.
    #[test]
    fn packetizer_handles_start_code_split_across_reads() {
        let mut packetizer = H264AnnexBPacketizer::new(Codec::H264);
        assert!(packetizer.push(&[0, 0]).is_empty());
        assert!(packetizer.push(&[0, 1, NAL_TYPE_IDR, 0x80]).is_empty());

        let out = packetizer.finish();
        assert_eq!(out.len(), 1);
        // No SPS/PPS seen, so IDR is emitted as-is
        assert_eq!(&out[0].data[..], &[0, 0, 0, 1, NAL_TYPE_IDR, 0x80]);
    }
    /// Several access units arriving in one read are split at their delimiters.
    #[test]
    fn packetizer_splits_multiple_access_units_in_one_buffer() {
        let mut packetizer = H264AnnexBPacketizer::new(Codec::H264);
        let mut data = nal(NAL_TYPE_AUD, &[0x10]);
        data.extend_from_slice(&nal(NAL_TYPE_IDR, &[0x80]));
        data.extend_from_slice(&nal(NAL_TYPE_AUD, &[0x10]));
        data.extend_from_slice(&nal(NAL_TYPE_NON_IDR, &[0x80]));

        let out = packetizer.push(&data);
        assert_eq!(out.len(), 1);
        // No SPS/PPS seen, so IDR AU emitted as-is
        assert_eq!(
            &out[0].data[..],
            &[
                0,
                0,
                0,
                1,
                NAL_TYPE_AUD,
                0x10,
                0,
                0,
                0,
                1,
                NAL_TYPE_IDR,
                0x80
            ]
        );

        let out = packetizer.finish();
        assert_eq!(out.len(), 1);
        assert_eq!(
            &out[0].data[..],
            &[
                0,
                0,
                0,
                1,
                NAL_TYPE_AUD,
                0x10,
                0,
                0,
                0,
                1,
                NAL_TYPE_NON_IDR,
                0x80
            ]
        );
    }
    /// SPS and PPS seen before an IDR are stored as codec config and prepended to the keyframe so a client can start decoding from it.
    #[test]
    fn packetizer_prepends_sps_pps_to_idr() {
        let mut packetizer = H264AnnexBPacketizer::new(Codec::H264);
        let mut data = nal(NAL_TYPE_SPS, &[0x64, 0x00]);
        data.extend_from_slice(&nal(NAL_TYPE_PPS, &[0xac]));
        data.extend_from_slice(&nal(NAL_TYPE_IDR, &[0x80]));

        assert!(packetizer.push(&data).is_empty());
        let config = packetizer.codec_config().expect("codec config");
        assert_eq!(
            &config[..],
            &[
                0,
                0,
                0,
                1,
                NAL_TYPE_SPS,
                0x64,
                0x00,
                0,
                0,
                0,
                1,
                NAL_TYPE_PPS,
                0xac
            ]
        );

        let out = packetizer.finish();
        assert_eq!(out.len(), 1);
        // IDR frame should now have SPS+PPS prepended
        assert_eq!(
            &out[0].data[..],
            &[
                // SPS
                0,
                0,
                0,
                1,
                NAL_TYPE_SPS,
                0x64,
                0x00,
                // PPS
                0,
                0,
                0,
                1,
                NAL_TYPE_PPS,
                0xac,
                // IDR
                0,
                0,
                0,
                1,
                NAL_TYPE_IDR,
                0x80
            ]
        );
    }
    /// A NAL still being received is never emitted; only complete NALs leave the packetizer.
    #[test]
    fn packetizer_does_not_emit_partial_nals() {
        let mut packetizer = H264AnnexBPacketizer::new(Codec::H264);
        let first = nal(NAL_TYPE_IDR, &[0x80, 0x11, 0x22]);

        assert!(packetizer.push(&first[..4]).is_empty());
        assert!(packetizer.push(&first[4..]).is_empty());

        let mut second = nal(NAL_TYPE_NON_IDR, &[0x80]);
        let out = packetizer.push(&second[..3]);
        assert!(out.is_empty());

        second.drain(..3);
        let out = packetizer.push(&second);
        assert!(out.is_empty());

        let out = packetizer.finish();
        assert_eq!(out.len(), 2);
        // No SPS/PPS seen, IDR emitted as-is
        assert_eq!(&out[0].data[..], &first[..]);
    }
}
