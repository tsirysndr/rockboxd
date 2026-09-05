//! Fragmented-MP4 (CMAF / DASH / HLS-fMP4) audio demuxer.
//!
//! Rockbox's MP4 codec path needs a complete `moov` sample table and a
//! seekable file, so fragmented MP4 (`moof`/`mdat` pairs) can't go through
//! it. Instead this module extracts the raw audio samples and re-frames them
//! as a self-describing bitstream the streaming codec path decodes directly:
//!
//! - **AAC** (`mp4a` + `esds`): each sample is one raw AAC frame — wrap it in
//!   a 7-byte ADTS header built from the AudioSpecificConfig, giving a plain
//!   `.aac` ADTS stream (decoder ext `"aac"`).
//! - **MP3** (`.mp3` sample entry): samples are already MPEG audio frames —
//!   concatenate (ext `"mp3"`).
//!
//! The **init segment** (`ftyp`+`moov`) configures the demuxer
//! ([`Fmp4Demuxer::init`]); each **media segment** (`styp`+`moof`+`mdat`…) is
//! then converted with [`Fmp4Demuxer::segment`]. Sample placement follows the
//! CMAF convention (`trun` data offsets relative to the `moof` start, or the
//! `mdat` payload when absent).

use std::io;

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// ADTS sampling-frequency table (index 0–12).
const FREQ_TABLE: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioKind {
    /// AAC with ADTS re-framing parameters from the AudioSpecificConfig.
    Aac {
        /// ADTS profile (audioObjectType − 1, i.e. 1 = AAC-LC).
        profile: u8,
        /// Core sampling-frequency index (0–12).
        freq_index: u8,
        /// Channel configuration (1–7).
        channels: u8,
    },
    Mp3,
}

/// Configured by an init segment; converts media segments to a raw bitstream.
#[derive(Debug)]
pub struct Fmp4Demuxer {
    kind: AudioKind,
    track_id: u32,
    /// `trex` default sample size (0 = none).
    trex_default_size: u32,
}

/// Does `data` start with an ISO-BMFF box that marks an fMP4 stream?
pub fn looks_like_mp4(data: &[u8]) -> bool {
    data.len() >= 8
        && matches!(
            &data[4..8],
            b"ftyp" | b"styp" | b"moov" | b"moof" | b"sidx" | b"prft" | b"emsg"
        )
}

/// Iterate the boxes directly contained in `data`: `(fourcc, box_start_offset,
/// payload)`. Tolerates a truncated trailing box by stopping.
fn boxes(data: &[u8]) -> impl Iterator<Item = ([u8; 4], usize, &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 8 > data.len() {
            return None;
        }
        let start = pos;
        let size32 = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as u64;
        let typ: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        let (size, hdr) = match size32 {
            0 => ((data.len() - pos) as u64, 8usize), // to end of buffer
            1 => {
                if pos + 16 > data.len() {
                    return None;
                }
                let s = u64::from_be_bytes(data[pos + 8..pos + 16].try_into().unwrap());
                (s, 16usize)
            }
            s => (s, 8usize),
        };
        if size < hdr as u64 || pos as u64 + size > data.len() as u64 {
            return None;
        }
        let payload = &data[pos + hdr..pos + size as usize];
        pos += size as usize;
        Some((typ, start, payload))
    })
}

fn find_box<'a>(data: &'a [u8], typ: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).find(|(t, _, _)| t == typ).map(|(_, _, p)| p)
}

fn be32(d: &[u8], off: usize) -> u32 {
    d.get(off..off + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .unwrap_or(0)
}

/// MPEG-4 descriptor length: 1–4 bytes, 7 bits each, MSB = continuation.
fn desc_len(d: &[u8], pos: &mut usize) -> usize {
    let mut len = 0usize;
    for _ in 0..4 {
        let Some(&b) = d.get(*pos) else { break };
        *pos += 1;
        len = (len << 7) | (b & 0x7F) as usize;
        if b & 0x80 == 0 {
            break;
        }
    }
    len
}

/// Extract the AudioSpecificConfig bytes from an `esds` box payload.
fn asc_from_esds(esds: &[u8]) -> Option<Vec<u8>> {
    let d = esds.get(4..)?; // skip version/flags
    let mut pos = 0usize;
    // ES_Descriptor (tag 0x03)
    if *d.get(pos)? != 0x03 {
        return None;
    }
    pos += 1;
    desc_len(d, &mut pos);
    pos += 2; // ES_ID
    let flags = *d.get(pos)?;
    pos += 1;
    if flags & 0x80 != 0 {
        pos += 2; // dependsOn_ES_ID
    }
    if flags & 0x40 != 0 {
        let url_len = *d.get(pos)? as usize;
        pos += 1 + url_len;
    }
    if flags & 0x20 != 0 {
        pos += 2; // OCR_ES_ID
    }
    // DecoderConfigDescriptor (tag 0x04)
    if *d.get(pos)? != 0x04 {
        return None;
    }
    pos += 1;
    desc_len(d, &mut pos);
    pos += 13; // objectTypeIndication, streamType, bufferSize, bitrates
               // DecoderSpecificInfo (tag 0x05) = AudioSpecificConfig
    if *d.get(pos)? != 0x05 {
        return None;
    }
    pos += 1;
    let len = desc_len(d, &mut pos);
    d.get(pos..pos + len).map(|s| s.to_vec())
}

/// Tiny MSB-first bit reader for the AudioSpecificConfig.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}
impl Bits<'_> {
    fn get(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.d.get(self.pos / 8)?;
            v = (v << 1) | ((byte >> (7 - self.pos % 8)) & 1) as u32;
            self.pos += 1;
        }
        Some(v)
    }
}

/// Parse an AudioSpecificConfig into ADTS parameters (profile, core
/// frequency index, channel configuration).
fn parse_asc(asc: &[u8]) -> io::Result<AudioKind> {
    let mut b = Bits { d: asc, pos: 0 };
    let read_aot = |b: &mut Bits| -> Option<u32> {
        let aot = b.get(5)?;
        if aot == 31 {
            Some(32 + b.get(6)?)
        } else {
            Some(aot)
        }
    };
    let bad = || err("malformed AudioSpecificConfig in fMP4 esds");
    let mut aot = read_aot(&mut b).ok_or_else(bad)?;
    let freq_index = b.get(4).ok_or_else(bad)?;
    if freq_index == 15 {
        b.get(24).ok_or_else(bad)?; // explicit rate — no ADTS index for it
        return Err(err(
            "fMP4 AAC uses an explicit sample rate, which ADTS cannot express",
        ));
    }
    let channels = b.get(4).ok_or_else(bad)?;
    if aot == 5 || aot == 29 {
        // Explicit SBR/PS signalling: the extension rate follows, then the
        // core object type. ADTS carries the core (LC) layer; the decoder
        // discovers SBR from the bitstream.
        let ext_freq = b.get(4).ok_or_else(bad)?;
        if ext_freq == 15 {
            b.get(24).ok_or_else(bad)?;
        }
        aot = read_aot(&mut b).ok_or_else(bad)?;
    }
    if !(1..=4).contains(&aot) {
        return Err(err(format!(
            "unsupported AAC object type {aot} in fMP4 stream"
        )));
    }
    if channels == 0 || channels > 7 {
        return Err(err(format!(
            "unsupported AAC channel configuration {channels}"
        )));
    }
    Ok(AudioKind::Aac {
        profile: (aot - 1) as u8,
        freq_index: freq_index as u8,
        channels: channels as u8,
    })
}

/// One run of samples: where their data starts and each sample's size.
struct SampleRun {
    data_start: usize,
    sizes: Vec<u32>,
}

impl Fmp4Demuxer {
    /// Parse an init segment (`moov`) and configure the demuxer for its audio
    /// track.
    pub fn init(data: &[u8]) -> io::Result<Self> {
        let moov = find_box(data, b"moov").ok_or_else(|| err("fMP4 init segment has no moov"))?;
        // Find the audio trak (hdlr type 'soun').
        let mut audio: Option<(u32, &[u8])> = None; // (track_id, stsd payload)
        let mut seen_entries: Vec<String> = Vec::new();
        for (typ, _, trak) in boxes(moov) {
            if &typ != b"trak" {
                continue;
            }
            let Some(mdia) = find_box(trak, b"mdia") else {
                continue;
            };
            let is_audio = find_box(mdia, b"hdlr")
                .and_then(|h| h.get(8..12))
                .is_some_and(|t| t == b"soun");
            if !is_audio {
                continue;
            }
            let Some(tkhd) = find_box(trak, b"tkhd") else {
                continue;
            };
            let track_id = match tkhd.first() {
                Some(1) => be32(tkhd, 20), // version 1: 64-bit times
                _ => be32(tkhd, 12),
            };
            let stsd = find_box(mdia, b"minf")
                .and_then(|m| find_box(m, b"stbl"))
                .and_then(|s| find_box(s, b"stsd"));
            if let Some(stsd) = stsd {
                audio = Some((track_id, stsd));
                break;
            }
        }
        let (track_id, stsd) = audio.ok_or_else(|| err("fMP4 init segment has no audio track"))?;

        // stsd payload: version/flags(4) entry_count(4), then sample entries.
        let entries = stsd.get(8..).unwrap_or(&[]);
        let mut kind: Option<AudioKind> = None;
        for (typ, _, entry) in boxes(entries).take(1) {
            let fourcc = String::from_utf8_lossy(&typ).into_owned();
            match &typ {
                b"mp4a" => {
                    // AudioSampleEntry: 28 fixed bytes, then child boxes.
                    let children = entry.get(28..).unwrap_or(&[]);
                    let esds = find_box(children, b"esds")
                        .ok_or_else(|| err("fMP4 mp4a sample entry has no esds decoder config"))?;
                    let asc = asc_from_esds(esds)
                        .ok_or_else(|| err("fMP4 esds has no AudioSpecificConfig"))?;
                    kind = Some(parse_asc(&asc)?);
                }
                b".mp3" | b"mp3 " => kind = Some(AudioKind::Mp3),
                _ => seen_entries.push(fourcc),
            }
        }
        let kind = kind.ok_or_else(|| {
            err(format!(
                "unsupported fMP4 audio codec ({}) — only AAC (mp4a) and MP3 are supported",
                seen_entries.join(", ")
            ))
        })?;

        // Optional mvex/trex default sample size for this track.
        let mut trex_default_size = 0u32;
        if let Some(mvex) = find_box(moov, b"mvex") {
            for (typ, _, trex) in boxes(mvex) {
                if &typ == b"trex" && be32(trex, 4) == track_id {
                    trex_default_size = be32(trex, 16);
                }
            }
        }

        Ok(Fmp4Demuxer {
            kind,
            track_id,
            trex_default_size,
        })
    }

    /// Decoder format extension for the extracted bitstream.
    pub fn ext(&self) -> &'static str {
        match self.kind {
            AudioKind::Aac { .. } => "aac",
            AudioKind::Mp3 => "mp3",
        }
    }

    /// Sample rate in Hz, when known (AAC only).
    pub fn sample_rate(&self) -> Option<u32> {
        match self.kind {
            AudioKind::Aac { freq_index, .. } => FREQ_TABLE.get(freq_index as usize).copied(),
            AudioKind::Mp3 => None,
        }
    }

    /// Convert one media segment (`moof`+`mdat`, possibly repeated for CMAF
    /// chunks), appending the re-framed bitstream to `out`.
    pub fn segment(&self, data: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        let mut pending: Vec<SampleRun> = Vec::new();
        for (typ, box_start, payload) in boxes(data) {
            match &typ {
                b"moof" => {
                    pending = self.parse_moof(payload, box_start)?;
                }
                b"mdat" => {
                    let mdat_payload_start = box_start + 8;
                    if pending.is_empty() {
                        continue;
                    }
                    for run in pending.drain(..) {
                        let start = if run.data_start > 0 {
                            run.data_start
                        } else {
                            mdat_payload_start
                        };
                        self.emit(data, start, &run.sizes, out)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Parse a `moof` payload into sample runs. `moof_start` is the box's
    /// offset in the segment buffer — `trun` data offsets are relative to it
    /// (CMAF `default-base-is-moof`).
    fn parse_moof(&self, moof: &[u8], moof_start: usize) -> io::Result<Vec<SampleRun>> {
        let mut runs = Vec::new();
        for (typ, _, traf) in boxes(moof) {
            if &typ != b"traf" {
                continue;
            }
            let Some(tfhd) = find_box(traf, b"tfhd") else {
                continue;
            };
            let tfhd_flags = be32(tfhd, 0) & 0x00FF_FFFF;
            if be32(tfhd, 4) != self.track_id {
                continue; // another (e.g. video) track's fragment
            }
            // Walk tfhd's optional fields to find default_sample_size.
            let mut off = 8usize;
            if tfhd_flags & 0x01 != 0 {
                off += 8; // base_data_offset (absolute; unsupported — see below)
            }
            if tfhd_flags & 0x02 != 0 {
                off += 4; // sample_description_index
            }
            if tfhd_flags & 0x08 != 0 {
                off += 4; // default_sample_duration
            }
            let tfhd_default_size = if tfhd_flags & 0x10 != 0 {
                be32(tfhd, off)
            } else {
                0
            };

            for (typ, _, trun) in boxes(traf) {
                if &typ != b"trun" {
                    continue;
                }
                let flags = be32(trun, 0) & 0x00FF_FFFF;
                let count = be32(trun, 4) as usize;
                let mut p = 8usize;
                let mut data_start = 0usize;
                if flags & 0x01 != 0 {
                    let data_offset = be32(trun, p) as i32;
                    p += 4;
                    let abs = moof_start as i64 + data_offset as i64;
                    if abs < 0 {
                        return Err(err("fMP4 trun data offset before segment start"));
                    }
                    data_start = abs as usize;
                }
                if flags & 0x04 != 0 {
                    p += 4; // first_sample_flags
                }
                let mut sizes = Vec::with_capacity(count);
                for _ in 0..count {
                    if flags & 0x100 != 0 {
                        p += 4; // sample_duration
                    }
                    let size = if flags & 0x200 != 0 {
                        let s = be32(trun, p);
                        p += 4;
                        s
                    } else if tfhd_default_size != 0 {
                        tfhd_default_size
                    } else {
                        self.trex_default_size
                    };
                    if flags & 0x400 != 0 {
                        p += 4; // sample_flags
                    }
                    if flags & 0x800 != 0 {
                        p += 4; // composition time offset
                    }
                    if size == 0 {
                        return Err(err("fMP4 sample size unknown (no trun/tfhd/trex size)"));
                    }
                    sizes.push(size);
                }
                runs.push(SampleRun { data_start, sizes });
            }
        }
        Ok(runs)
    }

    /// Write `sizes.len()` samples starting at `start` in `data` to `out`,
    /// framing per the audio kind.
    fn emit(&self, data: &[u8], start: usize, sizes: &[u32], out: &mut Vec<u8>) -> io::Result<()> {
        let mut pos = start;
        for &size in sizes {
            let size = size as usize;
            let sample = data
                .get(pos..pos + size)
                .ok_or_else(|| err("fMP4 sample data extends past the segment"))?;
            match self.kind {
                AudioKind::Aac {
                    profile,
                    freq_index,
                    channels,
                } => {
                    let frame_len = size + 7;
                    if frame_len >= 1 << 13 {
                        return Err(err("fMP4 AAC sample too large for an ADTS frame"));
                    }
                    out.extend_from_slice(&[
                        0xFF,
                        0xF1, // MPEG-4, layer 0, no CRC
                        (profile << 6) | (freq_index << 2) | (channels >> 2),
                        ((channels & 0x3) << 6) | ((frame_len >> 11) as u8),
                        ((frame_len >> 3) & 0xFF) as u8,
                        (((frame_len & 0x7) as u8) << 5) | 0x1F,
                        0xFC,
                    ]);
                    out.extend_from_slice(sample);
                }
                AudioKind::Mp3 => out.extend_from_slice(sample),
            }
            pos += size;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkbox(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(typ);
        v.extend_from_slice(payload);
        v
    }

    /// AAC-LC, 44.1 kHz (index 4), stereo — ASC 0x12 0x10.
    const ASC_LC_44K_STEREO: [u8; 2] = [0x12, 0x10];

    fn esds(asc: &[u8]) -> Vec<u8> {
        let mut dsi = vec![0x05, asc.len() as u8];
        dsi.extend_from_slice(asc);
        let mut dcd = vec![0x04, (13 + dsi.len()) as u8, 0x40, 0x15];
        dcd.extend_from_slice(&[0; 11]); // buffer size + bitrates
        dcd.extend_from_slice(&dsi);
        let mut es = vec![0x03, (3 + dcd.len()) as u8, 0x00, 0x01, 0x00];
        es.extend_from_slice(&dcd);
        let mut payload = vec![0, 0, 0, 0]; // version/flags
        payload.extend_from_slice(&es);
        payload
    }

    fn init_segment(asc: &[u8]) -> Vec<u8> {
        let mut mp4a = vec![0u8; 28]; // fixed AudioSampleEntry fields
        mp4a[6] = 0;
        mp4a.extend(mkbox(b"esds", &esds(asc)));
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1]; // version/flags, count=1
        stsd.extend(mkbox(b"mp4a", &mp4a));
        let stbl = mkbox(b"stsd", &stsd);
        let minf = mkbox(b"stbl", &stbl);
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"soun");
        hdlr.extend_from_slice(&[0u8; 13]);
        let mut mdia = mkbox(b"hdlr", &hdlr);
        mdia.extend(mkbox(b"minf", &minf));
        let mut tkhd = vec![0u8; 12]; // version 0 + ctime + mtime
        tkhd.extend_from_slice(&2u32.to_be_bytes()); // track_id = 2
        tkhd.extend_from_slice(&[0u8; 60]);
        let mut trak = mkbox(b"tkhd", &tkhd);
        trak.extend(mkbox(b"mdia", &mdia));
        let mut trex = vec![0u8; 4];
        trex.extend_from_slice(&2u32.to_be_bytes()); // track_id
        trex.extend_from_slice(&1u32.to_be_bytes()); // sample desc index
        trex.extend_from_slice(&1024u32.to_be_bytes()); // default duration
        trex.extend_from_slice(&0u32.to_be_bytes()); // default size
        let mvex = mkbox(b"trex", &trex);
        let mut moov_payload = mkbox(b"trak", &trak);
        moov_payload.extend(mkbox(b"mvex", &mvex));
        let mut out = mkbox(b"ftyp", b"isom\0\0\0\0isom");
        out.extend(mkbox(b"moov", &moov_payload));
        out
    }

    /// A moof+mdat media segment with the given sample sizes; sample bytes
    /// are sequential filler. Uses trun data_offset relative to moof start.
    fn media_segment(sizes: &[u32]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let samples: Vec<Vec<u8>> = sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| vec![i as u8 + 1; s as usize])
            .collect();
        let mdat_payload: Vec<u8> = samples.concat();

        let mut tfhd = 0x020000u32.to_be_bytes().to_vec(); // default-base-is-moof
        tfhd.extend_from_slice(&2u32.to_be_bytes()); // track_id

        let mut trun_payload = Vec::new();
        trun_payload.extend_from_slice(&(sizes.len() as u32).to_be_bytes());
        // data_offset placeholder — patched below.
        trun_payload.extend_from_slice(&0u32.to_be_bytes());
        for &s in sizes {
            trun_payload.extend_from_slice(&s.to_be_bytes());
        }
        let mut trun_flags_payload = 0x000201u32.to_be_bytes().to_vec(); // offset+size
        trun_flags_payload.extend_from_slice(&trun_payload);

        let trun = mkbox(b"trun", &trun_flags_payload);
        let mut traf_payload = mkbox(b"tfhd", &tfhd);
        traf_payload.extend(&trun);
        let mfhd = mkbox(b"mfhd", &[0, 0, 0, 0, 0, 0, 0, 1]);
        let mut moof_payload = mfhd;
        moof_payload.extend(mkbox(b"traf", &traf_payload));
        let mut moof = mkbox(b"moof", &moof_payload);
        // Patch trun data_offset: mdat payload starts at moof.len() + 8.
        let data_offset = (moof.len() + 8) as u32;
        let trun_pos = moof
            .windows(4)
            .position(|w| w == b"trun")
            .expect("trun in moof");
        let off_pos = trun_pos + 4 + 4 + 4; // fourcc, flags, sample_count
        moof[off_pos..off_pos + 4].copy_from_slice(&data_offset.to_be_bytes());

        let mut seg = moof;
        seg.extend(mkbox(b"mdat", &mdat_payload));
        (seg, samples)
    }

    #[test]
    fn init_parses_aac_config() {
        let demux = Fmp4Demuxer::init(&init_segment(&ASC_LC_44K_STEREO)).unwrap();
        assert_eq!(demux.ext(), "aac");
        assert_eq!(demux.sample_rate(), Some(44100));
        assert_eq!(demux.track_id, 2);
    }

    #[test]
    fn segment_wraps_samples_in_adts() {
        let demux = Fmp4Demuxer::init(&init_segment(&ASC_LC_44K_STEREO)).unwrap();
        let sizes = [100u32, 230, 7];
        let (seg, samples) = media_segment(&sizes);
        let mut out = Vec::new();
        demux.segment(&seg, &mut out).unwrap();

        // Expect one ADTS frame per sample.
        let mut pos = 0usize;
        for sample in &samples {
            let frame_len = sample.len() + 7;
            let hdr = &out[pos..pos + 7];
            assert_eq!(hdr[0], 0xFF);
            assert_eq!(hdr[1], 0xF1);
            // profile=LC(1), freq index 4, channels 2
            assert_eq!(hdr[2], (1 << 6) | (4 << 2));
            let parsed_len = (((hdr[3] & 0x3) as usize) << 11)
                | ((hdr[4] as usize) << 3)
                | ((hdr[5] >> 5) as usize);
            assert_eq!(parsed_len, frame_len);
            assert_eq!(&out[pos + 7..pos + frame_len], &sample[..]);
            pos += frame_len;
        }
        assert_eq!(pos, out.len());
    }

    #[test]
    fn he_aac_asc_uses_core_layer() {
        // aot=5 (SBR), core freq index 7 (22050), stereo, ext freq 4, core aot 2.
        // Bits: 00101 0111 0010 0100 00010 → 0x2B 0x92 0x08 (last byte padded).
        let asc = [0x2B, 0x92, 0x08, 0x00];
        match parse_asc(&asc).unwrap() {
            AudioKind::Aac {
                profile,
                freq_index,
                channels,
            } => {
                assert_eq!(profile, 1); // LC core
                assert_eq!(freq_index, 7); // 22050 Hz core rate
                assert_eq!(channels, 2);
            }
            other => panic!("unexpected kind {other:?}"),
        }
    }

    #[test]
    fn unsupported_codec_is_reported() {
        // Replace mp4a with an Opus sample entry.
        let mut init = init_segment(&ASC_LC_44K_STEREO);
        let pos = init.windows(4).position(|w| w == b"mp4a").unwrap();
        init[pos..pos + 4].copy_from_slice(b"Opus");
        let e = Fmp4Demuxer::init(&init).unwrap_err();
        assert!(e.to_string().contains("Opus"), "{e}");
    }

    #[test]
    fn mp4_sniffing() {
        assert!(looks_like_mp4(&mkbox(b"ftyp", b"isom")));
        assert!(looks_like_mp4(&mkbox(b"styp", b"msdh")));
        assert!(!looks_like_mp4(&[0x47; 200]));
        assert!(!looks_like_mp4(b"ID3\x03\0\0\0\0"));
    }
}
