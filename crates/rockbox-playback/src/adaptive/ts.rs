//! Minimal MPEG-TS (ISO 13818-1) demuxer for HLS segments: extract the audio
//! **elementary stream** out of 188-byte transport packets.
//!
//! HLS audio in `.ts` segments is (in practice) either ADTS AAC
//! (`stream_type 0x0F`) or MPEG-1 audio / MP3 (`0x03`/`0x04`) — both are
//! self-framing bitstreams the Rockbox codecs decode directly, so demuxing is
//! just: PAT → PMT → collect that PID's PES payloads, strip PES headers,
//! concatenate. Video PIDs and everything else are dropped.
//!
//! The demuxer is stateful across [`feed`](TsDemuxer::feed) calls: PAT/PMT
//! knowledge persists between segments, and packets/PES headers split across
//! feed boundaries are reassembled.

use std::io;

/// The audio codec found in the PMT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsAudioKind {
    /// ADTS AAC (`stream_type 0x0F`) — decoder ext `"aac"`.
    Aac,
    /// MPEG-1/2 audio layer I–III (`stream_type 0x03`/`0x04`) — ext `"mp3"`.
    Mpeg,
}

impl TsAudioKind {
    /// Decoder format extension for [`rockbox_codecs::Decoder::open_stream`].
    pub fn ext(self) -> &'static str {
        match self {
            TsAudioKind::Aac => "aac",
            TsAudioKind::Mpeg => "mp3",
        }
    }
}

const TS_PACKET: usize = 188;

#[derive(Default)]
pub struct TsDemuxer {
    pmt_pid: Option<u16>,
    audio_pid: Option<u16>,
    kind: Option<TsAudioKind>,
    /// Inside the selected PES packet, header already stripped — payloads
    /// stream straight to the output.
    in_pes: bool,
    /// PES bytes buffered until the (possibly split) header can be stripped.
    pes_hdr: Vec<u8>,
    /// Partial transport packet left over from the previous `feed`.
    leftover: Vec<u8>,
}

/// Quick check: does `data` start like an MPEG-TS stream (two sync bytes one
/// packet apart)?
pub fn looks_like_ts(data: &[u8]) -> bool {
    data.len() > TS_PACKET && data[0] == 0x47 && data[TS_PACKET] == 0x47
}

impl TsDemuxer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The audio codec, once the PMT has been seen.
    pub fn kind(&self) -> Option<TsAudioKind> {
        self.kind
    }

    /// Demux `data` (any slice of the transport stream), appending extracted
    /// elementary-stream audio bytes to `out`.
    pub fn feed(&mut self, data: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        let joined;
        let mut buf: &[u8] = if self.leftover.is_empty() {
            data
        } else {
            let mut j = std::mem::take(&mut self.leftover);
            j.extend_from_slice(data);
            joined = j;
            &joined
        };

        while buf.len() >= TS_PACKET {
            if buf[0] != 0x47 {
                // Lost sync — scan forward to the next plausible packet.
                match buf.iter().position(|&b| b == 0x47) {
                    Some(p) => {
                        buf = &buf[p..];
                        continue;
                    }
                    None => {
                        buf = &[];
                        break;
                    }
                }
            }
            self.packet(&buf[..TS_PACKET], out)?;
            buf = &buf[TS_PACKET..];
        }
        self.leftover = buf.to_vec();
        Ok(())
    }

    fn packet(&mut self, pkt: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        let pusi = pkt[1] & 0x40 != 0;
        let pid = (((pkt[1] & 0x1F) as u16) << 8) | pkt[2] as u16;
        let afc = (pkt[3] >> 4) & 0x3;
        let mut off = 4usize;
        if afc & 0x2 != 0 {
            // Adaptation field: length byte + that many bytes.
            let len = pkt[off] as usize;
            off += 1 + len;
            if off >= TS_PACKET {
                return Ok(());
            }
        }
        if afc & 0x1 == 0 {
            return Ok(()); // no payload
        }
        let payload = &pkt[off..];

        if pid == 0 {
            self.parse_pat(psi_section(payload, pusi));
            return Ok(());
        }
        if Some(pid) == self.pmt_pid && self.audio_pid.is_none() {
            self.parse_pmt(psi_section(payload, pusi))?;
            return Ok(());
        }
        if Some(pid) == self.audio_pid {
            self.audio_payload(payload, pusi, out);
        }
        Ok(())
    }

    fn parse_pat(&mut self, section: &[u8]) {
        // table_id, section_length, tsid, version/current, section numbers = 8
        // bytes, then 4-byte program entries, then CRC32.
        if section.len() < 12 || section[0] != 0x00 {
            return;
        }
        let section_len = (((section[1] & 0x0F) as usize) << 8) | section[2] as usize;
        let end = (3 + section_len).min(section.len()).saturating_sub(4); // strip CRC
        let mut i = 8;
        while i + 4 <= end {
            let program = ((section[i] as u16) << 8) | section[i + 1] as u16;
            let pid = (((section[i + 2] & 0x1F) as u16) << 8) | section[i + 3] as u16;
            if program != 0 {
                self.pmt_pid = Some(pid);
                return;
            }
            i += 4;
        }
    }

    fn parse_pmt(&mut self, section: &[u8]) -> io::Result<()> {
        if section.len() < 16 || section[0] != 0x02 {
            return Ok(());
        }
        let section_len = (((section[1] & 0x0F) as usize) << 8) | section[2] as usize;
        let end = (3 + section_len).min(section.len()).saturating_sub(4); // strip CRC
        let program_info_len = (((section[10] & 0x0F) as usize) << 8) | section[11] as usize;
        let mut i = 12 + program_info_len;
        let mut saw_latm = false;
        while i + 5 <= end {
            let stream_type = section[i];
            let pid = (((section[i + 1] & 0x1F) as u16) << 8) | section[i + 2] as u16;
            let es_info_len = (((section[i + 3] & 0x0F) as usize) << 8) | section[i + 4] as usize;
            i += 5 + es_info_len;
            let kind = match stream_type {
                0x0F => Some(TsAudioKind::Aac),
                0x03 | 0x04 => Some(TsAudioKind::Mpeg),
                0x11 => {
                    saw_latm = true;
                    None
                }
                _ => None,
            };
            if let Some(kind) = kind {
                self.audio_pid = Some(pid);
                self.kind = Some(kind);
                return Ok(());
            }
        }
        if saw_latm {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "MPEG-TS carries AAC-LATM audio (stream_type 0x11), which is not supported",
            ));
        }
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no supported audio stream (ADTS AAC or MPEG audio) in the MPEG-TS program",
        ))
    }

    fn audio_payload(&mut self, payload: &[u8], pusi: bool, out: &mut Vec<u8>) {
        if pusi {
            self.in_pes = false;
            self.pes_hdr.clear();
            self.pes_hdr.extend_from_slice(payload);
        } else if !self.pes_hdr.is_empty() {
            self.pes_hdr.extend_from_slice(payload);
        } else if self.in_pes {
            out.extend_from_slice(payload);
            return;
        } else {
            return; // payload before any PES start — drop
        }
        // Try to strip the (possibly still incomplete) PES header:
        // 00 00 01 | stream_id | packet_length(2) | flags(2) | header_len(1)
        if self.pes_hdr.len() < 9 {
            return;
        }
        if self.pes_hdr[0] != 0 || self.pes_hdr[1] != 0 || self.pes_hdr[2] != 1 {
            self.pes_hdr.clear(); // not a PES start — resync on next PUSI
            return;
        }
        let header_len = self.pes_hdr[8] as usize;
        if self.pes_hdr.len() < 9 + header_len {
            return; // header spans another packet
        }
        out.extend_from_slice(&self.pes_hdr[9 + header_len..]);
        self.pes_hdr.clear();
        self.in_pes = true;
    }
}

/// PSI payload → section bytes (skip the pointer field on a unit start).
fn psi_section(payload: &[u8], pusi: bool) -> &[u8] {
    if pusi && !payload.is_empty() {
        let ptr = payload[0] as usize;
        payload.get(1 + ptr..).unwrap_or(&[])
    } else {
        payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one 188-byte TS packet.
    fn ts_packet(pid: u16, pusi: bool, payload: &[u8]) -> Vec<u8> {
        assert!(payload.len() <= 184);
        let mut p = vec![0u8; TS_PACKET];
        p[0] = 0x47;
        p[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
        p[2] = pid as u8;
        if payload.len() == 184 {
            p[3] = 0x10; // payload only
            p[4..].copy_from_slice(payload);
        } else {
            // Pad with an adaptation field so the payload fills to the end.
            p[3] = 0x30; // adaptation + payload
            let af_len = 184 - payload.len() - 1;
            p[4] = af_len as u8;
            if af_len > 0 {
                p[5] = 0x00;
                for b in &mut p[6..5 + af_len] {
                    *b = 0xFF;
                }
            }
            p[188 - payload.len()..].copy_from_slice(payload);
        }
        p
    }

    fn psi(table: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8]; // pointer field
        v.extend_from_slice(table);
        v
    }

    /// PAT with a single program pointing at `pmt_pid`.
    fn pat(pmt_pid: u16) -> Vec<u8> {
        let mut s = vec![
            0x00, // table_id
            0xB0,
            0x0D, // section_length = 13
            0x00,
            0x01, // tsid
            0xC1,
            0x00,
            0x00, // version, section numbers
            0x00,
            0x01, // program 1
            0xE0 | (pmt_pid >> 8) as u8,
            pmt_pid as u8,
        ];
        s.extend_from_slice(&[0, 0, 0, 0]); // CRC (unchecked)
        s
    }

    /// PMT declaring one video and one audio stream.
    fn pmt(audio_type: u8, audio_pid: u16) -> Vec<u8> {
        let mut s = vec![
            0x02, // table_id
            0xB0,
            0x17, // section_length = 23
            0x00,
            0x01, // program
            0xC1,
            0x00,
            0x00, // version, section numbers
            0xE1,
            0x00, // PCR pid
            0xF0,
            0x00, // program_info_length = 0
            // video stream first — must be skipped
            0x1B,
            0xE1,
            0x00,
            0xF0,
            0x00,
            // audio stream
            audio_type,
            0xE0 | (audio_pid >> 8) as u8,
            audio_pid as u8,
            0xF0,
            0x00,
        ];
        s.extend_from_slice(&[0, 0, 0, 0]); // CRC
        s
    }

    /// PES packet bytes (header + payload).
    fn pes(payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0x00, 0x00, 0x01, 0xC0]; // audio stream_id
        let len = 3 + 5 + payload.len(); // flags+hdr_len + PTS + payload
        v.push((len >> 8) as u8);
        v.push(len as u8);
        v.extend_from_slice(&[0x80, 0x80, 0x05]); // flags, PTS present, hdr len 5
        v.extend_from_slice(&[0x21, 0x00, 0x01, 0x00, 0x01]); // dummy PTS
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn demuxes_adts_audio_across_packets() {
        let audio_pid = 0x101;
        let es: Vec<u8> = (0u8..=255).cycle().take(400).collect();
        let pes = pes(&es);

        let mut ts = Vec::new();
        ts.extend(ts_packet(0, true, &psi(&pat(0x100))));
        ts.extend(ts_packet(0x100, true, &psi(&pmt(0x0F, audio_pid))));
        // Audio PES split across three packets.
        ts.extend(ts_packet(audio_pid, true, &pes[..184]));
        ts.extend(ts_packet(audio_pid, false, &pes[184..368]));
        ts.extend(ts_packet(audio_pid, false, &pes[368..]));
        // A video packet that must be ignored.
        ts.extend(ts_packet(0x100 + 0x50, true, &[0xAA; 184]));

        let mut demux = TsDemuxer::new();
        let mut out = Vec::new();
        demux.feed(&ts, &mut out).unwrap();
        assert_eq!(demux.kind(), Some(TsAudioKind::Aac));
        assert_eq!(out, es);
    }

    #[test]
    fn handles_split_feeds_and_mpeg_audio() {
        let audio_pid = 0x44;
        let es = vec![0x5A; 300];
        let pes = pes(&es);

        let mut ts = Vec::new();
        ts.extend(ts_packet(0, true, &psi(&pat(0x20))));
        ts.extend(ts_packet(0x20, true, &psi(&pmt(0x03, audio_pid))));
        ts.extend(ts_packet(audio_pid, true, &pes[..184]));
        ts.extend(ts_packet(audio_pid, false, &pes[184..]));

        let mut demux = TsDemuxer::new();
        let mut out = Vec::new();
        // Feed in awkward chunk sizes so packets span feed() calls.
        for chunk in ts.chunks(101) {
            demux.feed(chunk, &mut out).unwrap();
        }
        assert_eq!(demux.kind(), Some(TsAudioKind::Mpeg));
        assert_eq!(out, es);
    }

    #[test]
    fn pmt_without_audio_errors() {
        let mut section = vec![
            0x02, 0xB0, 0x12, 0x00, 0x01, 0xC1, 0x00, 0x00, 0xE1, 0x00, 0xF0, 0x00, 0x1B, 0xE1,
            0x00, 0xF0, 0x00, // video only
        ];
        section.extend_from_slice(&[0, 0, 0, 0]);
        let mut ts = Vec::new();
        ts.extend(ts_packet(0, true, &psi(&pat(0x30))));
        ts.extend(ts_packet(0x30, true, &psi(&section)));
        let mut demux = TsDemuxer::new();
        let mut out = Vec::new();
        assert!(demux.feed(&ts, &mut out).is_err());
    }

    #[test]
    fn ts_sniffing() {
        let mut ts = vec![0u8; 2 * TS_PACKET];
        ts[0] = 0x47;
        ts[TS_PACKET] = 0x47;
        assert!(looks_like_ts(&ts));
        assert!(!looks_like_ts(&[0x47; 100]));
        ts[TS_PACKET] = 0x00;
        assert!(!looks_like_ts(&ts));
    }
}
