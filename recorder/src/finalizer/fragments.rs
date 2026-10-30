//! Independent timing proof for the native recorder's empty-moov fragmented MP4.
//! Native output has no B frames: decode order must also be presentation order.
//! Only headers/sample tables are read, in fixed-size chunks; media is skipped.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use chronobreak_replay_time::{MAX_REPLAY_DURATION_SECONDS, Rational};

const MAX_BOXES: usize = 1_000_000;

#[derive(Clone, Copy)]
struct BoxHeader {
    kind: [u8; 4],
    start: u64,
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
            start: offset,
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

    fn video_track(&mut self, moov: BoxHeader, rate: Rational) -> Result<VideoTiming> {
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
            let mut position = trak.payload;
            let mut edit_seen = false;
            while position < trak.end {
                let child = self.header(position, trak.end)?;
                if child.kind == *b"edts" {
                    ensure!(!edit_seen, "duplicate MP4 video edit box");
                    edit_seen = true;
                    let elst = self.child(child, *b"elst")?;
                    let header = self.bytes::<8>(elst.payload, elst.end)?;
                    ensure!(
                        header[1..4] == [0; 3] && header[4..] == [0, 0, 0, 1],
                        "unsupported MP4 video edit list"
                    );
                    let rate = match header[0] {
                        0 if elst.end - elst.payload == 20 => {
                            ensure!(
                                self.bytes::<8>(elst.payload + 8, elst.end)? == [0; 8],
                                "nonidentity MP4 video edit"
                            );
                            self.bytes::<4>(elst.payload + 16, elst.end)?
                        }
                        1 if elst.end - elst.payload == 28 => {
                            ensure!(
                                self.bytes::<16>(elst.payload + 8, elst.end)? == [0; 16],
                                "nonidentity MP4 video edit"
                            );
                            self.bytes::<4>(elst.payload + 24, elst.end)?
                        }
                        _ => anyhow::bail!("unsupported MP4 video edit list"),
                    };
                    // delay_moov emits one open-ended, zero-offset, rate-one
                    // edit. It leaves the fragment presentation grid unchanged.
                    ensure!(rate == [0, 1, 0, 0], "nonidentity MP4 video edit rate");
                }
                position = child.end;
            }
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
            let mut position = stbl.payload;
            while position < stbl.end {
                let table = self.header(position, stbl.end)?;
                if matches!(&table.kind, b"stts" | b"ctts" | b"stsc" | b"stco" | b"co64") {
                    ensure!(
                        u32::from_be_bytes(self.bytes(table.payload + 4, table.end)?) == 0,
                        "nonempty initial MP4 timing/chunk table"
                    );
                }
                position = table.end;
            }
            let mdhd = self.child(mdia, *b"mdhd")?;
            let version = self.bytes::<4>(mdhd.payload, mdhd.end)?;
            ensure!(
                version == [0; 4] || version == [1, 0, 0, 0],
                "unsupported MP4 media header"
            );
            let scale_offset = if version[0] == 1 { 20 } else { 12 };
            let timescale = u32::from_be_bytes(self.bytes(mdhd.payload + scale_offset, mdhd.end)?);
            let numerator = u64::try_from(rate.numerator())?;
            ensure!(numerator > 0 && timescale > 0, "invalid MP4 video cadence");
            let product = u64::from(timescale)
                .checked_mul(rate.denominator())
                .context("MP4 cadence overflow")?;
            ensure!(
                product % numerator == 0,
                "MP4 timescale cannot represent exact video cadence"
            );
            let step = product / numerator;
            ensure!(step > 0, "invalid MP4 sample duration");
            video_id = Some(VideoTiming {
                id,
                timescale,
                step,
                samples: 0,
                end_pts: 0,
            });
        }
        video_id.context("MP4 video track missing")
    }

    fn defaults(&mut self, moov: BoxHeader, id: u32) -> Result<(u32, u32)> {
        let mvex = self.child(moov, *b"mvex")?;
        let mut offset = mvex.payload;
        let mut defaults = None;
        while offset < mvex.end {
            let atom = self.header(offset, mvex.end)?;
            offset = atom.end;
            if atom.kind != *b"trex" {
                continue;
            }
            let bytes = self.bytes::<24>(atom.payload, atom.end)?;
            ensure!(
                atom.end - atom.payload == 24 && bytes[..4] == [0; 4],
                "invalid MP4 trex"
            );
            if u32::from_be_bytes(bytes[4..8].try_into()?) == id {
                ensure!(defaults.is_none(), "duplicate MP4 video trex");
                ensure!(
                    u32::from_be_bytes(bytes[8..12].try_into()?) == 1,
                    "unsupported MP4 sample description"
                );
                defaults = Some((
                    u32::from_be_bytes(bytes[12..16].try_into()?),
                    u32::from_be_bytes(bytes[16..20].try_into()?),
                ));
            }
        }
        defaults.context("MP4 video trex missing")
    }

    fn track_fragment(
        &mut self,
        traf: BoxHeader,
        moof: BoxHeader,
        mdat: BoxHeader,
        timing: &mut VideoTiming,
        defaults: (u32, u32),
    ) -> Result<()> {
        let tfhd = self.child(traf, *b"tfhd")?;
        let header = self.bytes::<8>(tfhd.payload, tfhd.end)?;
        let flags = u32::from_be_bytes(header[..4].try_into()?);
        ensure!(flags & !0x03003b == 0, "unsupported MP4 tfhd flags/version");
        let extra = u64::from(flags & 1 != 0) * 8 + u64::from((flags & 0x3a).count_ones()) * 4;
        ensure!(
            tfhd.end - tfhd.payload == 8 + extra,
            "invalid MP4 tfhd size"
        );
        if u32::from_be_bytes(header[4..].try_into()?) != timing.id {
            return Ok(());
        }
        ensure!(flags & 0x010000 == 0, "empty MP4 video fragment");
        let mut field = tfhd.payload + 8;
        let base = if flags & 1 != 0 {
            let base = u64::from_be_bytes(self.bytes(field, tfhd.end)?);
            field += 8;
            base
        } else {
            ensure!(flags & 0x020000 != 0, "ambiguous MP4 fragment data base");
            moof.start
        };
        if flags & 2 != 0 {
            ensure!(
                u32::from_be_bytes(self.bytes(field, tfhd.end)?) == 1,
                "unsupported MP4 sample description"
            );
            field += 4;
        }
        let duration = if flags & 8 != 0 {
            let value = u32::from_be_bytes(self.bytes(field, tfhd.end)?);
            field += 4;
            value
        } else {
            defaults.0
        };
        let size = if flags & 16 != 0 {
            u32::from_be_bytes(self.bytes(field, tfhd.end)?)
        } else {
            defaults.1
        };
        let tfdt = self.child(traf, *b"tfdt")?;
        let version = self.bytes::<4>(tfdt.payload, tfdt.end)?;
        let start = match version {
            [0, 0, 0, 0] if tfdt.end - tfdt.payload == 8 => {
                u64::from(u32::from_be_bytes(self.bytes(tfdt.payload + 4, tfdt.end)?))
            }
            [1, 0, 0, 0] if tfdt.end - tfdt.payload == 12 => {
                u64::from_be_bytes(self.bytes(tfdt.payload + 4, tfdt.end)?)
            }
            _ => anyhow::bail!("unsupported or malformed MP4 tfdt"),
        };
        ensure!(
            start == timing.end_pts,
            "MP4 video decode/presentation grid is discontinuous"
        );
        let mut offset = traf.payload;
        let mut data_end = None;
        let mut runs = 0;
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
            ensure!(count > 0, "empty MP4 video run");
            let next_end = timing
                .end_pts
                .checked_add(
                    count
                        .checked_mul(timing.step)
                        .context("MP4 duration overflow")?,
                )
                .context("MP4 timestamp overflow")?;
            ensure!(
                next_end <= u64::from(timing.timescale) * MAX_REPLAY_DURATION_SECONDS,
                "MP4 video exceeds maximum replay duration"
            );
            let prefix = 8 + u64::from((full_flags & 5).count_ones()) * 4;
            let entry_size = u64::from((full_flags & 0xf00).count_ones()) * 4;
            ensure!(
                trun.end - trun.payload == prefix + count * entry_size,
                "truncated or malformed MP4 sample table"
            );
            ensure!(full_flags & 0x404 != 0x404, "conflicting MP4 sample flags");
            let data_start = if full_flags & 1 != 0 {
                let displacement = i32::from_be_bytes(self.bytes(trun.payload + 8, trun.end)?);
                base.checked_add_signed(i64::from(displacement))
                    .context("MP4 data offset overflow")?
            } else {
                data_end.context("MP4 first video run has no data offset")?
            };
            ensure!(
                data_start >= mdat.payload && data_start < mdat.end,
                "MP4 video data outside mdat"
            );
            let mut total_size = 0_u64;
            let mut position = trun.payload + prefix;
            // Common native runs inherit one duration and have no composition
            // offsets. Establish their whole grid algebraically, without a
            // per-sample loop unless explicit table fields require inspection.
            if full_flags & 0x100 == 0 {
                ensure!(
                    u64::from(duration) == timing.step,
                    "MP4 video sample duration is off the CFR grid"
                );
            }
            if entry_size == 0 {
                ensure!(size > 0, "MP4 video sample size missing");
                total_size = count
                    .checked_mul(u64::from(size))
                    .context("MP4 sample bytes overflow")?;
            } else {
                let stride = usize::try_from(entry_size)?;
                let mut remaining = count;
                let mut buffer = [0_u8; 4096];
                while remaining > 0 {
                    let entries = remaining.min(u64::try_from(buffer.len() / stride)?);
                    let length = usize::try_from(entries * entry_size)?;
                    ensure!(
                        Instant::now() < self.deadline,
                        "MP4 fragment scan timed out"
                    );
                    self.input.seek(SeekFrom::Start(position))?;
                    self.input.read_exact(&mut buffer[..length])?;
                    for entry in buffer[..length].chunks_exact(stride) {
                        let mut cursor = 0;
                        let mut next = || {
                            let value = u32::from_be_bytes(
                                entry[cursor..cursor + 4]
                                    .try_into()
                                    .expect("validated table stride"),
                            );
                            cursor += 4;
                            value
                        };
                        if full_flags & 0x100 != 0 {
                            ensure!(
                                u64::from(next()) == timing.step,
                                "MP4 video sample duration is off the CFR grid"
                            );
                        }
                        let sample_size = if full_flags & 0x200 != 0 {
                            next()
                        } else {
                            size
                        };
                        ensure!(sample_size > 0, "MP4 video sample size missing");
                        total_size = total_size
                            .checked_add(u64::from(sample_size))
                            .context("MP4 sample bytes overflow")?;
                        if full_flags & 0x400 != 0 {
                            next();
                        }
                        if full_flags & 0x800 != 0 {
                            ensure!(
                                next() == 0,
                                "MP4 video composition offset violates native presentation order"
                            );
                        }
                    }
                    position += u64::try_from(length)?;
                    remaining -= entries;
                }
            }
            let end = data_start
                .checked_add(total_size)
                .context("MP4 sample range overflow")?;
            ensure!(end <= mdat.end, "MP4 video samples exceed mdat");
            if let Some(previous) = data_end {
                ensure!(data_start >= previous, "overlapping MP4 video sample runs");
            }
            data_end = Some(end);
            timing.samples = timing
                .samples
                .checked_add(count)
                .context("MP4 sample count overflow")?;
            timing.end_pts = next_end;
            runs += 1;
        }
        ensure!(runs > 0, "MP4 video fragment has no sample runs");
        Ok(())
    }

    fn inspect(&mut self, length: u64, rate: Rational) -> Result<VideoTiming> {
        let mut offset = 0;
        let mut video_id = None;
        let mut defaults = None;
        while offset < length {
            let atom = self.header(offset, length)?;
            offset = atom.end;
            match &atom.kind {
                b"moov" => {
                    ensure!(video_id.is_none(), "duplicate MP4 movie header");
                    let timing = self.video_track(atom, rate)?;
                    defaults = Some(self.defaults(atom, timing.id)?);
                    video_id = Some(timing);
                }
                b"moof" => {
                    let timing = video_id
                        .as_mut()
                        .context("MP4 fragment precedes movie header")?;
                    let mdat = self.header(offset, length)?;
                    ensure!(
                        mdat.kind == *b"mdat" && mdat.end > mdat.payload,
                        "MP4 fragment has no complete media data box"
                    );
                    offset = mdat.end;
                    let mut position = atom.payload;
                    while position < atom.end {
                        let traf = self.header(position, atom.end)?;
                        position = traf.end;
                        if traf.kind == *b"traf" {
                            self.track_fragment(
                                traf,
                                atom,
                                mdat,
                                timing,
                                defaults.context("MP4 video defaults missing")?,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
        let timing = video_id.context("MP4 video track missing")?;
        ensure!(timing.samples > 0, "MP4 has no complete video samples");
        Ok(timing)
    }
}

#[derive(Debug)]
pub(super) struct VideoTiming {
    id: u32,
    pub timescale: u32,
    step: u64,
    pub samples: u64,
    pub end_pts: u64,
}

pub(super) fn video_sample_timing(
    path: &Path,
    deadline: Instant,
    rate: Rational,
) -> Result<VideoTiming> {
    let input = File::open(path).context("could not open fragmented MP4")?;
    let length = input.metadata()?.len();
    let mut reader = Reader {
        input,
        deadline,
        boxes: 0,
    };
    let timing = reader.inspect(length, rate)?;
    ensure!(
        reader.input.metadata()?.len() == length,
        "MP4 changed during finalization"
    );
    Ok(timing)
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
        let mut mdhd = vec![0; 12];
        mdhd.extend_from_slice(&15_360_u32.to_be_bytes());
        let mut stsz = vec![0; 8];
        stsz.extend_from_slice(&initial_count.to_be_bytes());
        let stbl = atom(b"stbl", &atom(b"stsz", &stsz));
        let mdia = atom(
            b"mdia",
            &[
                atom(b"hdlr", &hdlr),
                atom(b"mdhd", &mdhd),
                atom(b"minf", &stbl),
            ]
            .concat(),
        );
        atom(b"trak", &[atom(b"tkhd", &tkhd), mdia].concat())
    }

    fn movie(tracks: &[u8]) -> Vec<u8> {
        let trex = [0_u32, 7, 1, 256, 1, 0]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect::<Vec<_>>();
        atom(
            b"moov",
            &[tracks.to_vec(), atom(b"mvex", &atom(b"trex", &trex))].concat(),
        )
    }

    fn traf(id: u32, count: u32) -> Vec<u8> {
        let tfhd = atom(
            b"tfhd",
            &[0x020000_u32.to_be_bytes(), id.to_be_bytes()].concat(),
        );
        let tfdt = atom(b"tfdt", &[0; 8]);
        let trun = atom(
            b"trun",
            &[
                1_u32.to_be_bytes(),
                count.to_be_bytes(),
                0_u32.to_be_bytes(),
            ]
            .concat(),
        );
        atom(b"traf", &[tfhd, tfdt, trun].concat())
    }

    fn fragment(mut moof: Vec<u8>, start: u32) -> Vec<u8> {
        let length = u32::try_from(moof.len()).unwrap();
        // These synthetic fixtures use one video run; audio is ignored.
        for offset in 0..moof.len() - 4 {
            if &moof[offset..offset + 4] == b"tfdt" {
                moof[offset + 8..offset + 12].copy_from_slice(&start.to_be_bytes());
            }
            if &moof[offset..offset + 4] == b"trun" {
                moof[offset + 12..offset + 16].copy_from_slice(&(length + 8).to_be_bytes());
            }
        }
        [moof, atom(b"mdat", &[1; 128])].concat()
    }

    fn fixture() -> Vec<u8> {
        let moov = movie(&[track(7, b"vide", 0), track(2, b"soun", 0)].concat());
        let moof = atom(b"moof", &[traf(2, 99), traf(7, 60)].concat());
        [moov, fragment(moof.clone(), 0), fragment(moof, 15_360)].concat()
    }

    fn count(bytes: Vec<u8>) -> Result<u64> {
        let length = bytes.len() as u64;
        Reader {
            input: Cursor::new(bytes),
            deadline: Instant::now() + Duration::from_secs(2),
            boxes: 0,
        }
        .inspect(length, Rational::new(60, 1).unwrap())
        .map(|timing| timing.samples)
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
        let moov = movie(&track(7, b"vide", 0));
        assert!(count([moov.clone(), atom(b"moof", &traf(7, 60))].concat()).is_err());
        assert!(
            count(
                [
                    movie(&[track(7, b"vide", 0), track(8, b"vide", 0)].concat()),
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
                    movie(&track(7, b"vide", 10)),
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
        let moov = movie(&track(7, b"vide", 0));
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
                fragment(
                    atom(
                        b"moof",
                        &atom(
                            b"traf",
                            &[tfhd.clone(), atom(b"tfdt", &[0; 8]), atom(b"trun", data)].concat(),
                        ),
                    ),
                    0,
                ),
            ]
            .concat()
        };
        assert_eq!(count(make(&trun)).unwrap(), 2);
        // Same sample count and aggregate duration; individual cadence is wrong.
        let mut perturbed = trun.clone();
        perturbed[12..16].copy_from_slice(&255_u32.to_be_bytes());
        perturbed[16..20].copy_from_slice(&257_u32.to_be_bytes());
        assert!(
            count(make(&perturbed))
                .unwrap_err()
                .to_string()
                .contains("sample duration")
        );
        trun.pop();
        assert!(count(make(&trun)).is_err());
    }

    #[test]
    fn rejects_discontinuous_origins_composition_offsets_and_missing_defaults() {
        let mut discontinuous = fixture();
        let at = discontinuous
            .windows(4)
            .rposition(|v| v == b"tfdt")
            .unwrap();
        discontinuous[at + 8..at + 12].copy_from_slice(&1_u32.to_be_bytes());
        assert!(
            count(discontinuous)
                .unwrap_err()
                .to_string()
                .contains("discontinuous")
        );
        let mut missing = fixture();
        let at = missing.windows(4).rposition(|v| v == b"tfdt").unwrap();
        missing[at..at + 4].copy_from_slice(b"free");
        assert!(count(missing).is_err());
        let mut defaults = fixture();
        let at = defaults.windows(4).position(|v| v == b"trex").unwrap();
        defaults[at + 16..at + 20].copy_from_slice(&0_u32.to_be_bytes());
        assert!(
            count(defaults)
                .unwrap_err()
                .to_string()
                .contains("sample duration")
        );
        for composition in [0_u32, 1, u32::MAX] {
            let tfhd = atom(
                b"tfhd",
                &[0x020018_u32, 7, 256, 1]
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect::<Vec<_>>(),
            );
            let trun = atom(
                b"trun",
                &[0x01000801_u32, 2, 0, 0, composition]
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect::<Vec<_>>(),
            );
            let bytes = [
                movie(&track(7, b"vide", 0)),
                fragment(
                    atom(
                        b"moof",
                        &atom(b"traf", &[tfhd, atom(b"tfdt", &[0; 8]), trun].concat()),
                    ),
                    0,
                ),
            ]
            .concat();
            assert_eq!(count(bytes).is_ok(), composition == 0);
        }
        // Exercise the exact edit-list boundary emitted by delay_moov: only
        // the open-ended identity edit preserves the proven presentation grid.
        for fields in [
            [0_u32, 1, 0, 0, 0x00010000],
            [0, 1, 1, 0, 0x00010000],
            [0, 1, 0, 1, 0x00010000],
            [0, 1, 0, 0, 0x00020000],
        ] {
            let mut trak = track(7, b"vide", 0);
            let edit = atom(
                b"elst",
                &fields
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect::<Vec<_>>(),
            );
            trak.extend_from_slice(&atom(b"edts", &edit));
            let length = u32::try_from(trak.len()).unwrap();
            trak[..4].copy_from_slice(&length.to_be_bytes());
            let bytes = [movie(&trak), fragment(atom(b"moof", &traf(7, 60)), 0)].concat();
            assert_eq!(count(bytes).is_ok(), fields == [0, 1, 0, 0, 0x00010000]);
        }
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
        let mut bytes = [movie(&track(7, b"vide", 0)), atom(b"moof", &traf(7, 120))].concat();
        let trun = bytes.windows(4).position(|value| value == b"trun").unwrap();
        let moof = bytes.windows(4).position(|value| value == b"moof").unwrap() - 4;
        let data_offset = u32::try_from(bytes.len() - moof + 16).unwrap();
        bytes[trun + 12..trun + 16].copy_from_slice(&data_offset.to_be_bytes());
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(b"mdat");
        bytes.extend_from_slice(&(payload_size + 16).to_be_bytes());
        let header_length = bytes.len() as u64;
        let mut reader = Reader {
            input: HeaderOnly(Cursor::new(bytes), header_length),
            deadline: Instant::now() + Duration::from_secs(2),
            boxes: 0,
        };
        assert_eq!(
            reader
                .inspect(header_length + payload_size, Rational::new(60, 1).unwrap())
                .unwrap()
                .samples,
            120
        );
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
        assert!(
            reader
                .inspect(length, Rational::new(60, 1).unwrap())
                .is_err()
        );
        reader.deadline = Instant::now() + Duration::from_secs(2);
        reader.boxes = MAX_BOXES;
        assert!(
            reader
                .inspect(length, Rational::new(60, 1).unwrap())
                .is_err()
        );
    }
}
