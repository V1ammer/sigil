//! Remux an MP4 (H.264 + optional AAC) into a **fragmented** MP4 (fMP4) suitable
//! for MediaSource streaming.
//!
//! Android WebView (and our MSE driver) can't play a non-fragmented MP4: `<video>`
//! can't range-stream our encrypted blob, and MediaSource only accepts fragmented
//! MP4. Phone videos are almost always plain (non-fragmented) MP4/MOV with
//! H.264/AAC. We re-mux them on the **sender** — stream-copy the samples (no
//! re-encode) into one fMP4 with keyframe-aligned fragments — so every recipient
//! streams the result via `video/mp4; codecs="…"`, exactly like voice/audio.
//!
//! Returns `None` for inputs we can't remux (no H.264 track, parse failure, …);
//! the caller then keeps the original bytes (which fall back to whole-blob play).

use std::io::Cursor;

use mp4::{MediaType, Mp4Reader, TrackType};
use mse_fmp4::aac::{AacProfile, ChannelConfiguration, SamplingFrequency};
use mse_fmp4::avc::AvcDecoderConfigurationRecord;
use mse_fmp4::fmp4::{
    AacSampleEntry, AvcConfigurationBox, AvcSampleEntry, InitializationSegment, MediaDataBox,
    MediaSegment, MovieExtendsHeaderBox, Mp4Box, Mpeg4EsDescriptorBox, Sample, SampleEntry,
    SampleFlags, TrackBox, TrackExtendsBox, TrackFragmentBox,
};
use mse_fmp4::io::WriteTo;

/// Target fragment length in seconds (fragments still snap to keyframes).
const FRAG_SECONDS: f64 = 2.0;

struct VideoCfg {
    id: u32,
    timescale: u32,
    width: u16,
    height: u16,
    sps: Vec<u8>,
    pps: Vec<u8>,
}

struct AudioCfg {
    id: u32,
    timescale: u32,
    freq_index: u8,
    channels: u8,
    object_type: u8,
}

/// One decoded track's samples + concatenated sample data.
struct Samples {
    samples: Vec<Sample>,
    data: Vec<u8>,
    /// Sync (keyframe) flag parallel to `samples` (meaningful for video).
    sync: Vec<bool>,
}

/// Remux `input` (an MP4) to fragmented MP4. Returns `(fmp4_bytes, mime)` where
/// `mime` is `video/mp4; codecs="…"`, or `None` if the input can't be remuxed.
#[must_use]
pub fn remux_to_fmp4(input: &[u8]) -> Option<(Vec<u8>, String)> {
    let len = input.len() as u64;
    let mut reader = Mp4Reader::read_header(Cursor::new(input), len).ok()?;

    // 1. Identify tracks + collect configs (immutable borrow) before reading samples.
    let mut vcfg: Option<VideoCfg> = None;
    let mut acfg: Option<AudioCfg> = None;
    for (id, tr) in reader.tracks() {
        match tr.track_type() {
            Ok(TrackType::Video) if matches!(tr.media_type(), Ok(MediaType::H264)) => {
                vcfg = Some(VideoCfg {
                    id: *id,
                    timescale: tr.timescale(),
                    width: tr.width(),
                    height: tr.height(),
                    sps: tr.sequence_parameter_set().ok()?.to_vec(),
                    pps: tr.picture_parameter_set().ok()?.to_vec(),
                });
            }
            Ok(TrackType::Audio) if matches!(tr.media_type(), Ok(MediaType::AAC)) => {
                acfg = Some(AudioCfg {
                    id: *id,
                    timescale: tr.timescale(),
                    freq_index: tr.sample_freq_index().ok()? as u8,
                    channels: tr.channel_config().ok()? as u8,
                    object_type: tr.audio_profile().ok()? as u8,
                });
            }
            _ => {}
        }
    }
    let vcfg = vcfg?; // a video track is required

    // profile/constraint/level from the SPS (byte 0 is the NAL header).
    if vcfg.sps.len() < 4 {
        return None;
    }
    let (profile_idc, constraint, level_idc) = (vcfg.sps[1], vcfg.sps[2], vcfg.sps[3]);

    // 2. Read all samples.
    let video = read_track_samples(&mut reader, vcfg.id, true)?;
    let audio = match &acfg {
        Some(a) => Some(read_track_samples(&mut reader, a.id, false)?),
        None => None,
    };

    let vid_dur: u64 = video.samples.iter().filter_map(|s| s.duration).map(u64::from).sum();
    let aud_dur: u64 = audio
        .as_ref()
        .map(|a| a.samples.iter().filter_map(|s| s.duration).map(u64::from).sum())
        .unwrap_or(0);
    let vid_dur_u32 = u32::try_from(vid_dur).unwrap_or(u32::MAX);
    let aud_dur_u32 = u32::try_from(aud_dur).unwrap_or(u32::MAX);

    // 3. Initialization segment (ftyp + moov with mvex).
    let mut init = InitializationSegment::default();
    init.moov_box.mvhd_box.timescale = vcfg.timescale;
    init.moov_box.mvhd_box.duration = vid_dur_u32;
    init.moov_box.mvex_box.mehd_box = Some(MovieExtendsHeaderBox {
        fragment_duration: vid_dur_u32,
    });

    let mut vtrak = TrackBox::new(true);
    vtrak.tkhd_box.width = u32::from(vcfg.width) << 16;
    vtrak.tkhd_box.height = u32::from(vcfg.height) << 16;
    vtrak.tkhd_box.duration = vid_dur_u32;
    vtrak.mdia_box.mdhd_box.timescale = vcfg.timescale;
    vtrak.mdia_box.mdhd_box.duration = vid_dur_u32;
    vtrak
        .mdia_box
        .minf_box
        .stbl_box
        .stsd_box
        .sample_entries
        .push(SampleEntry::Avc(AvcSampleEntry {
            width: vcfg.width,
            height: vcfg.height,
            avcc_box: AvcConfigurationBox {
                configuration: AvcDecoderConfigurationRecord {
                    profile_idc,
                    constraint_set_flag: constraint,
                    level_idc,
                    sequence_parameter_set: vcfg.sps.clone(),
                    picture_parameter_set: vcfg.pps.clone(),
                },
            },
        }));
    init.moov_box.trak_boxes.push(vtrak);
    init.moov_box.mvex_box.trex_boxes.push(TrackExtendsBox::new(true));

    if let Some(a) = &acfg {
        let mut atrak = TrackBox::new(false);
        atrak.tkhd_box.duration = aud_dur_u32;
        atrak.mdia_box.mdhd_box.timescale = a.timescale;
        atrak.mdia_box.mdhd_box.duration = aud_dur_u32;
        atrak
            .mdia_box
            .minf_box
            .stbl_box
            .stsd_box
            .sample_entries
            .push(SampleEntry::Aac(AacSampleEntry {
                esds_box: Mpeg4EsDescriptorBox {
                    profile: map_profile(a.object_type),
                    frequency: map_freq(a.freq_index)?,
                    channel_configuration: map_channels(a.channels),
                },
            }));
        init.moov_box.trak_boxes.push(atrak);
        init.moov_box.mvex_box.trex_boxes.push(TrackExtendsBox::new(false));
    }

    let mut out = Vec::with_capacity(input.len() + 64 * 1024);
    init.write_to(&mut out).ok()?;

    // 4. Media segments, snapped to video keyframes (~FRAG_SECONDS each).
    let frag_ticks = (FRAG_SECONDS * f64::from(vcfg.timescale)) as u64;
    let boundaries = fragment_boundaries(&video, frag_ticks);

    let mut seq = 1u32;
    let mut v_off = 0usize; // sample index into video
    let mut a_off = 0usize; // sample index into audio
    let mut v_time = 0u64; // accumulated video ticks emitted
    for &v_end in &boundaries {
        let v_slice: Vec<Sample> = video.samples[v_off..v_end].to_vec();
        let v_data = video.data[data_offset(&video, v_off)..data_offset(&video, v_end)].to_vec();
        let v_dur: u64 = v_slice.iter().filter_map(|s| s.duration).map(u64::from).sum();

        // Audio samples whose timeline falls within the video fragment's span.
        let (a_payload, a_consumed) = match (&acfg, &audio) {
            (Some(a), Some(au)) => {
                let a_end = audio_cut(au, a_off, v_time + v_dur, vcfg.timescale, a.timescale);
                let slice: Vec<Sample> = au.samples[a_off..a_end].to_vec();
                let data = au.data[data_offset(au, a_off)..data_offset(au, a_end)].to_vec();
                (Some((slice, data)), a_end - a_off)
            }
            _ => (None, 0),
        };

        write_media_segment(&mut out, seq, v_slice, v_data, a_payload)?;

        v_off = v_end;
        v_time += v_dur;
        a_off += a_consumed;
        seq += 1;
    }

    let codecs = if acfg.is_some() {
        format!(
            "avc1.{profile_idc:02x}{constraint:02x}{level_idc:02x}, mp4a.40.{}",
            acfg.as_ref().map_or(2, |a| a.object_type)
        )
    } else {
        format!("avc1.{profile_idc:02x}{constraint:02x}{level_idc:02x}")
    };
    Some((out, format!("video/mp4; codecs=\"{codecs}\"")))
}

/// Read every sample of a track into fMP4 `Sample`s + concatenated data.
fn read_track_samples(
    reader: &mut Mp4Reader<Cursor<&[u8]>>,
    track_id: u32,
    is_video: bool,
) -> Option<Samples> {
    let count = reader.sample_count(track_id).ok()?;
    let mut samples = Vec::with_capacity(count as usize);
    let mut data = Vec::new();
    let mut sync = Vec::with_capacity(count as usize);
    for i in 1..=count {
        let s = reader.read_sample(track_id, i).ok()??;
        let size = s.bytes.len() as u32;
        let flags = if is_video {
            Some(SampleFlags {
                is_leading: 0,
                sample_depends_on: if s.is_sync { 2 } else { 1 },
                sample_is_depdended_on: 0,
                sample_has_redundancy: 0,
                sample_padding_value: 0,
                sample_is_non_sync_sample: !s.is_sync,
                sample_degradation_priority: 0,
            })
        } else {
            None
        };
        samples.push(Sample {
            duration: Some(s.duration),
            size: Some(size),
            flags,
            composition_time_offset: if is_video { Some(s.rendering_offset) } else { None },
        });
        sync.push(s.is_sync);
        data.extend_from_slice(&s.bytes);
    }
    Some(Samples { samples, data, sync })
}

/// Exclusive video-sample indices at which to cut fragments: each fragment runs
/// up to (but not including) the next keyframe past `frag_ticks` of content.
fn fragment_boundaries(video: &Samples, frag_ticks: u64) -> Vec<usize> {
    let n = video.samples.len();
    let mut bounds = Vec::new();
    let mut acc = 0u64;
    for i in 0..n {
        if i > 0 && video.sync[i] && acc >= frag_ticks.max(1) {
            bounds.push(i);
            acc = 0;
        }
        acc += u64::from(video.samples[i].duration.unwrap_or(0));
    }
    bounds.push(n);
    bounds
}

/// First audio-sample index past `frag_end_video_ticks`, converted to the audio
/// timescale — so audio fragments track the video fragment span.
fn audio_cut(audio: &Samples, start: usize, frag_end_video_ticks: u64, v_ts: u32, a_ts: u32) -> usize {
    let target = frag_end_video_ticks * u64::from(a_ts) / u64::from(v_ts.max(1));
    let mut acc = 0u64;
    for i in 0..start {
        acc += u64::from(audio.samples[i].duration.unwrap_or(0));
    }
    let mut idx = start;
    while idx < audio.samples.len() && acc < target {
        acc += u64::from(audio.samples[idx].duration.unwrap_or(0));
        idx += 1;
    }
    idx
}

/// Byte offset into `s.data` of sample index `i` (sum of preceding sample sizes).
fn data_offset(s: &Samples, i: usize) -> usize {
    s.samples[..i].iter().filter_map(|x| x.size).map(|v| v as usize).sum()
}

/// Append one media segment (moof + mdat(s)) to `out`.
fn write_media_segment(
    out: &mut Vec<u8>,
    seq: u32,
    v_samples: Vec<Sample>,
    v_data: Vec<u8>,
    audio: Option<(Vec<Sample>, Vec<u8>)>,
) -> Option<()> {
    let mut seg = MediaSegment::default();
    seg.moof_box.mfhd_box.sequence_number = seq;

    let mut vtraf = TrackFragmentBox::new(true);
    vtraf.trun_box.data_offset = Some(0);
    vtraf.trun_box.samples = v_samples;
    seg.moof_box.traf_boxes.push(vtraf);

    if let Some((a_samples, _)) = &audio {
        let mut atraf = TrackFragmentBox::new(false);
        atraf.trun_box.data_offset = Some(0);
        atraf.trun_box.samples = a_samples.clone();
        seg.moof_box.traf_boxes.push(atraf);
    }

    // Fix up trun data offsets (relative to the moof start). The offset fields
    // are fixed-size, so measuring the moof with placeholder offsets is exact.
    let mut scratch = Vec::new();
    seg.moof_box.write_box(&mut scratch).ok()?;
    let moof_len = i32::try_from(scratch.len()).ok()?;
    let v_len = i32::try_from(v_data.len()).ok()?;

    seg.moof_box.traf_boxes[0].trun_box.data_offset = Some(moof_len + 8);
    seg.mdat_boxes.push(MediaDataBox { data: v_data });

    if let Some((_, a_data)) = audio {
        // After moof + video mdat (8-byte header + v_len), the audio mdat's
        // payload begins another 8 bytes in.
        seg.moof_box.traf_boxes[1].trun_box.data_offset = Some(moof_len + 8 + v_len + 8);
        seg.mdat_boxes.push(MediaDataBox { data: a_data });
    }

    seg.write_to(out).ok()
}

fn map_profile(object_type: u8) -> AacProfile {
    match object_type {
        1 => AacProfile::Main,
        3 => AacProfile::Ssr,
        4 => AacProfile::Ltp,
        _ => AacProfile::Lc, // 2 = AAC-LC, the common case
    }
}

fn map_channels(channels: u8) -> ChannelConfiguration {
    match channels {
        1 => ChannelConfiguration::OneChannel,
        3 => ChannelConfiguration::ThreeChannels,
        4 => ChannelConfiguration::FourChannels,
        5 => ChannelConfiguration::FiveChannels,
        6 => ChannelConfiguration::SixChannels,   // 5.1
        7 => ChannelConfiguration::EightChannels, // 7.1
        _ => ChannelConfiguration::TwoChannels,   // 2 = stereo, the common case
    }
}

/// Map an AAC sampling-frequency index (0–12) to the fMP4 enum.
fn map_freq(index: u8) -> Option<SamplingFrequency> {
    use SamplingFrequency::{
        Hz11025, Hz12000, Hz16000, Hz22050, Hz24000, Hz32000, Hz44100, Hz48000, Hz64000, Hz7350,
        Hz8000, Hz88200, Hz96000,
    };
    Some(match index {
        0 => Hz96000,
        1 => Hz88200,
        2 => Hz64000,
        3 => Hz48000,
        4 => Hz44100,
        5 => Hz32000,
        6 => Hz24000,
        7 => Hz22050,
        8 => Hz16000,
        9 => Hz12000,
        10 => Hz11025,
        11 => Hz8000,
        12 => Hz7350,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remux_test_mp4_if_present() {
        // Run only when a sample file is staged (native dev convenience).
        let Ok(input) = std::fs::read("/tmp/test.mp4") else {
            return;
        };
        let (fmp4, mime) = remux_to_fmp4(&input).expect("remux");
        std::fs::write("/tmp/test.fmp4", &fmp4).unwrap();
        eprintln!("in={} out={} mime={mime}", input.len(), fmp4.len());
        assert!(mime.starts_with("video/mp4; codecs=\"avc1."));
        // Output must itself parse as a valid MP4.
        let r = mp4::Mp4Reader::read_header(std::io::Cursor::new(&fmp4[..]), fmp4.len() as u64);
        assert!(r.is_ok(), "remuxed output should parse");
    }
}
