//! Unit tests of `packetizer.rs`. Moved out of the file so the code and its tests read separately; they see its private items through `use super::*`.

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
