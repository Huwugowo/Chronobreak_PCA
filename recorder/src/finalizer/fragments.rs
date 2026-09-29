//! Exact sample counts for the recorder's empty-moov fragmented MP4 output.
//! Only box headers and fixed table fields are read; media payload is skipped.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};

const MAX_BOXES: usize = 1_000_000;

#[derive(Clone, Copy)]
struct BoxHeader {
    kind: [u8; 4],
    payload: u64,
    end: u64,
}

struct Reader<R> {
    input: R,
    deadline: Instant,
    boxes: usize,
}

impl<R: Read + Seek> Reader<R> {
    fn bytes<const N: usize>(&mut self, offset: u64, end: u64) -> Result<[u8; N]> {
        ensure!(
            Instant::now() < self.deadline,
            "MP4 fragment scan timed out"
        );
        ensure!(
            offset <= end && N as u64 <= end - offset,
            "truncated MP4 field"
        );
        self.input.seek(SeekFrom::Start(offset))?;
        let mut bytes = [0; N];
        self.input.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn header(&mut self, offset: u64, end: u64) -> Result<BoxHeader> {
        self.boxes += 1;
        ensure!(self.boxes <= MAX_BOXES, "MP4 fragment box limit exceeded");
        let bytes = self.bytes::<8>(offset, end)?;
        let size = u64::from(u32::from_be_bytes(bytes[..4].try_into()?));
        let (size, header_size) = match size {
            0 => (end - offset, 8),
            1 => (u64::from_be_bytes(self.bytes(offset + 8, end)?), 16),
            size => (size, 8),
        };
        ensure!(
            size >= header_size && size <= end - offset,
            "invalid MP4 box size"
        );
        Ok(BoxHeader {
            kind: bytes[4..].try_into()?,
            payload: offset + header_size,
            end: offset + size,
        })
    }

    fn child(&mut self, parent: BoxHeader, kind: [u8; 4]) -> Result<BoxHeader> {
        let mut offset = parent.payload;
        let mut found = None;
        while offset < parent.end {
            let child = self.header(offset, parent.end)?;
            if child.kind == kind {
                ensure!(found.is_none(), "duplicate MP4 child box");
                found = Some(child);
            }
            offset = child.end;
        }
        found.context("required MP4 child box missing")
    }

    fn video_track(&mut self, moov: BoxHeader) -> Result<u32> {
        let mut offset = moov.payload;
        let mut video_id = None;
        while offset < moov.end {
            let trak = self.header(offset, moov.end)?;
            offset = trak.end;
            if trak.kind != *b"trak" {
                continue;
            }
            let mdia = self.child(trak, *b"mdia")?;
            let hdlr = self.child(mdia, *b"hdlr")?;
            if self.bytes::<4>(hdlr.payload + 8, hdlr.end)? != *b"vide" {
                continue;
            }
            ensure!(video_id.is_none(), "multiple MP4 video tracks");
            let tkhd = self.child(trak, *b"tkhd")?;
            let version = self.bytes::<4>(tkhd.payload, tkhd.end)?[0];
            ensure!(version <= 1, "unsupported MP4 track header version");
            let id_offset = if version == 1 { 20 } else { 12 };
            let id = u32::from_be_bytes(self.bytes(tkhd.payload + id_offset, tkhd.end)?);
            ensure!(id > 0, "invalid MP4 track ID");
            let minf = self.child(mdia, *b"minf")?;
            let stbl = self.child(minf, *b"stbl")?;
            let stsz = self.child(stbl, *b"stsz")?;
            let initial_count = u32::from_be_bytes(self.bytes(stsz.payload + 8, stsz.end)?);
            ensure!(
                initial_count == 0,
                "fragment count requires an empty initial sample table"
            );
            video_id = Some(id);
        }
        video_id.context("MP4 video track missing")
    }

    fn track_fragment(&mut self, traf: BoxHeader, video_id: u32) -> Result<u64> {
        let tfhd = self.child(traf, *b"tfhd")?;
        let header = self.bytes::<8>(tfhd.payload, tfhd.end)?;
        let flags = u32::from_be_bytes(header[..4].try_into()?);
        ensure!(flags & !0x03003b == 0, "unsupported MP4 tfhd flags/version");
        let extra = u64::from(flags & 1 != 0) * 8 + u64::from((flags & 0x3a).count_ones()) * 4;
        ensure!(
            tfhd.end - tfhd.payload == 8 + extra,
            "invalid MP4 tfhd size"
        );
        if u32::from_be_bytes(header[4..].try_into()?) != video_id {
            return Ok(0);
        }
        let mut offset = traf.payload;
        let mut samples = 0_u64;
        while offset < traf.end {
            let trun = self.header(offset, traf.end)?;
            offset = trun.end;
            if trun.kind != *b"trun" {
                continue;
            }
            let header = self.bytes::<8>(trun.payload, trun.end)?;
            let full_flags = u32::from_be_bytes(header[..4].try_into()?);
            ensure!(
                full_flags >> 24 <= 1 && full_flags & 0x00ff_f0fa == 0,
                "unsupported MP4 trun flags/version"
            );
            let count = u64::from(u32::from_be_bytes(header[4..].try_into()?));
            let prefix = 8 + u64::from((full_flags & 5).count_ones()) * 4;
            let entry_size = u64::from((full_flags & 0xf00).count_ones()) * 4;
            ensure!(
                trun.end - trun.payload == prefix + count * entry_size,
                "truncated or malformed MP4 sample table"
            );
            samples = samples
                .checked_add(count)
                .context("MP4 sample count overflow")?;
        }
        Ok(samples)
    }

    fn count(&mut self, length: u64) -> Result<u64> {
        let mut offset = 0;
        let mut video_id = None;
        let mut samples = 0_u64;
        let mut pending = None;
        while offset < length {
            let atom = self.header(offset, length)?;
            offset = atom.end;
            if let Some(count) = pending.take() {
                ensure!(
                    atom.kind == *b"mdat" && atom.end > atom.payload,
                    "MP4 fragment has no complete media data box"
                );
                samples = samples
                    .checked_add(count)
                    .context("MP4 sample count overflow")?;
                continue;
            }
            match &atom.kind {
                b"moov" => {
                    ensure!(video_id.is_none(), "duplicate MP4 movie header");
                    video_id = Some(self.video_track(atom)?);
                }
                b"moof" => {
                    let id = video_id.context("MP4 fragment precedes movie header")?;
                    let mut position = atom.payload;
                    let mut count = 0_u64;
                    while position < atom.end {
                        let traf = self.header(position, atom.end)?;
                        position = traf.end;
                        if traf.kind == *b"traf" {
                            count = count
                                .checked_add(self.track_fragment(traf, id)?)
                                .context("MP4 sample count overflow")?;
                        }
                    }
                    pending = Some(count);
                }
                _ => {}
            }
        }
        ensure!(pending.is_none(), "MP4 ends before fragment media data");
        ensure!(samples > 0, "MP4 has no complete video samples");
        Ok(samples)
    }
}

pub(super) fn video_sample_count(path: &Path, deadline: Instant) -> Result<u64> {
    let input = File::open(path).context("could not open fragmented MP4")?;
    let length = input.metadata()?.len();
    let mut reader = Reader {
        input,
        deadline,
        boxes: 0,
    };
    let count = reader.count(length)?;
    ensure!(
        reader.input.metadata()?.len() == length,
        "MP4 changed during finalization"
    );
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration;

    fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn track(id: u32, kind: &[u8; 4], initial_count: u32) -> Vec<u8> {
        let mut tkhd = vec![0; 12];
        tkhd.extend_from_slice(&id.to_be_bytes());
        let mut hdlr = vec![0; 8];
        hdlr.extend_from_slice(kind);
        let mut stsz = vec![0; 8];
        stsz.extend_from_slice(&initial_count.to_be_bytes());
        let stbl = atom(b"stbl", &atom(b"stsz", &stsz));
        let mdia = atom(
            b"mdia",
            &[atom(b"hdlr", &hdlr), atom(b"minf", &stbl)].concat(),
        );
        atom(b"trak", &[atom(b"tkhd", &tkhd), mdia].concat())
    }

    fn traf(id: u32, count: u32) -> Vec<u8> {
        let tfhd = atom(b"tfhd", &[0_u32.to_be_bytes(), id.to_be_bytes()].concat());
        let trun = atom(
            b"trun",
            &[0_u32.to_be_bytes(), count.to_be_bytes()].concat(),
        );
        atom(b"traf", &[tfhd, trun].concat())
    }

    fn fixture() -> Vec<u8> {
        let moov = atom(
            b"moov",
            &[track(7, b"vide", 0), track(2, b"soun", 0)].concat(),
        );
        let moof = atom(b"moof", &[traf(2, 99), traf(7, 60)].concat());
        [
            moov,
            moof.clone(),
            atom(b"mdat", &[1; 64]),
            moof,
            atom(b"mdat", &[2; 64]),
        ]
        .concat()
    }

    fn count(bytes: Vec<u8>) -> Result<u64> {
        let length = bytes.len() as u64;
        Reader {
            input: Cursor::new(bytes),
            deadline: Instant::now() + Duration::from_secs(2),
            boxes: 0,
        }
        .count(length)
    }

    #[test]
    fn sums_only_video_samples_across_fragments() {
        assert_eq!(count(fixture()).unwrap(), 120);
    }

    #[test]
    fn rejects_truncation_missing_data_ambiguous_tracks_and_nonempty_initial_tables() {
        let mut truncated = fixture();
        truncated.pop();
        assert!(count(truncated).is_err());
        let moov = atom(b"moov", &track(7, b"vide", 0));
        assert!(count([moov.clone(), atom(b"moof", &traf(7, 60))].concat()).is_err());
        assert!(
            count(
                [
                    atom(
                        b"moov",
                        &[track(7, b"vide", 0), track(8, b"vide", 0)].concat()
                    ),
                    atom(b"moof", &traf(7, 60)),
                    atom(b"mdat", &[1])
                ]
                .concat()
            )
            .is_err()
        );
        assert!(
            count(
                [
                    atom(b"moov", &track(7, b"vide", 10)),
                    atom(b"moof", &traf(7, 60)),
                    atom(b"mdat", &[1])
                ]
                .concat()
            )
            .is_err()
        );
        assert!(count([moov, atom(b"moof", &traf(8, 60)), atom(b"mdat", &[1])].concat()).is_err());
    }

    #[test]
    fn validates_optional_sample_table_fields_and_declared_lengths() {
        let moov = atom(b"moov", &track(7, b"vide", 0));
        let tfhd = atom(
            b"tfhd",
            &[0x020000_u32.to_be_bytes(), 7_u32.to_be_bytes()].concat(),
        );
        // One signed data offset and two per-sample durations.
        let mut trun = [
            0x101_u32.to_be_bytes(),
            2_u32.to_be_bytes(),
            0_u32.to_be_bytes(),
            256_u32.to_be_bytes(),
            256_u32.to_be_bytes(),
        ]
        .concat();
        let make = |data: &[u8]| {
            [
                moov.clone(),
                atom(
                    b"moof",
                    &atom(b"traf", &[tfhd.clone(), atom(b"trun", data)].concat()),
                ),
                atom(b"mdat", &[1]),
            ]
            .concat()
        };
        assert_eq!(count(make(&trun)).unwrap(), 2);
        trun.pop();
        assert!(count(make(&trun)).is_err());
    }

    #[test]
    fn skips_large_media_payload_and_handles_extended_box_sizes() {
        struct HeaderOnly(Cursor<Vec<u8>>, u64);
        impl Read for HeaderOnly {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                assert!(self.0.position() < self.1, "read media payload");
                self.0.read(bytes)
            }
        }
        impl Seek for HeaderOnly {
            fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
                self.0.seek(position)
            }
        }
        let payload_size = 8_u64 * 1024 * 1024 * 1024;
        let mut bytes = [
            atom(b"moov", &track(7, b"vide", 0)),
            atom(b"moof", &traf(7, 120)),
        ]
        .concat();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(b"mdat");
        bytes.extend_from_slice(&(payload_size + 16).to_be_bytes());
        let header_length = bytes.len() as u64;
        let mut reader = Reader {
            input: HeaderOnly(Cursor::new(bytes), header_length),
            deadline: Instant::now() + Duration::from_secs(2),
            boxes: 0,
        };
        assert_eq!(reader.count(header_length + payload_size).unwrap(), 120);
    }

    #[test]
    fn enforces_deadline_and_box_budget() {
        let bytes = fixture();
        let length = bytes.len() as u64;
        let mut reader = Reader {
            input: Cursor::new(bytes),
            deadline: Instant::now(),
            boxes: 0,
        };
        assert!(reader.count(length).is_err());
        reader.deadline = Instant::now() + Duration::from_secs(2);
        reader.boxes = MAX_BOXES;
        assert!(reader.count(length).is_err());
    }
}
