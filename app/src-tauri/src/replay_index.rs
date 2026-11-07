//! Read the native fragmented MP4 layout and build a metadata-only playback prefix.

use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use chronobreak_replay_time::{MediaTimelineV2, REPLAY_TICKS_PER_SECOND};

const MAX_BOX_BYTES: usize = 256 * 1024;
const MAX_READ_BYTES: usize = 64 * 1024 * 1024;
const MAX_REFERENCES: usize = u16::MAX as usize;
const MAX_PREPARATION: Duration = Duration::from_millis(750);

type Kind = [u8; 4];

pub(crate) struct ReplayIndex {
    pub prefix: Bytes,
    pub source_start: u64,
    pub length: u64,
}

struct Header {
    kind: Kind,
    size: u64,
    header: usize,
}

fn header(bytes: &[u8]) -> Option<Header> {
    let small = number(bytes, 0)?;
    let kind = bytes.get(4..8)?.try_into().ok()?;
    let (size, header) = if small == 1 {
        (u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?), 16)
    } else {
        (u64::from(small), 8)
    };
    (size >= header as u64).then_some(Header { kind, size, header })
}

fn next_box<'a>(bytes: &mut &'a [u8]) -> Option<(Kind, &'a [u8])> {
    let item = header(bytes)?;
    let (this, rest) = bytes.split_at_checked(usize::try_from(item.size).ok()?)?;
    *bytes = rest;
    Some((item.kind, &this[item.header..]))
}

fn fields<const N: usize>(mut bytes: &[u8], kinds: [Kind; N]) -> Option<[Option<&[u8]>; N]> {
    let mut result = [None; N];
    while !bytes.is_empty() {
        let (kind, value) = next_box(&mut bytes)?;
        let slot = &mut result[kinds.iter().position(|item| *item == kind)?];
        if slot.replace(value).is_some() {
            return None;
        }
    }
    Some(result)
}

fn number(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn versioned(bytes: &[u8], v0: usize, v1: usize) -> Option<u32> {
    number(
        bytes,
        match *bytes.first()? {
            0 => v0,
            1 => v1,
            _ => return None,
        },
    )
}

#[derive(Default, Clone, Copy)]
struct Defaults {
    duration: u32,
    size: u32,
    flags: u32,
}

struct Track {
    id: u32,
    timescale: u32,
    defaults: Defaults,
    end: u64,
    frames: u64,
    refs: Vec<(u32, u32)>,
}

fn track(bytes: &[u8]) -> Option<(bool, Track)> {
    let [tkhd, edts, mdia] = fields(bytes, [*b"tkhd", *b"edts", *b"mdia"])?;
    let [mdhd, hdlr, minf] = fields(mdia?, [*b"mdhd", *b"hdlr", *b"minf"])?;
    let video = match hdlr?.get(8..12)? {
        b"vide" => true,
        b"soun" => false,
        _ => return None,
    };
    if let Some(edit) = edts {
        let [elst] = fields(edit, [*b"elst"])?;
        let elst = elst?;
        let media_time = match elst {
            [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, a, b, c, d, 0, 1, 0, 0]
            | [
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                a,
                b,
                c,
                d,
                0,
                1,
                0,
                0,
            ] => u32::from_be_bytes([*a, *b, *c, *d]),
            _ => return None,
        };
        if media_time != 0 && (video || media_time != 1024) {
            return None;
        }
    }

    let [_, _, dinf, stbl] = fields(minf?, [*b"vmhd", *b"smhd", *b"dinf", *b"stbl"])?;
    let [dref] = fields(dinf?, [*b"dref"])?;
    // One self-contained URL entry. Fragment offsets must refer to this file.
    if dref? != b"\0\0\0\0\0\0\0\x01\0\0\0\x0curl \0\0\0\x01" {
        return None;
    }
    let [stsd, stsz, stts, stsc, stco, co64, ctts, stss] = fields(
        stbl?,
        [
            *b"stsd", *b"stsz", *b"stts", *b"stsc", *b"stco", *b"co64", *b"ctts", *b"stss",
        ],
    )?;
    if stsz? != [0; 12]
        || stts? != [0; 8]
        || stsc? != [0; 8]
        || stco.or(co64)? != [0; 8]
        || (stco.is_some() && co64.is_some())
        || [ctts, stss]
            .into_iter()
            .flatten()
            .any(|table| table != [0; 8])
    {
        return None;
    }
    let stsd = stsd?;
    let mut entries = stsd.strip_prefix(b"\0\0\0\0\0\0\0\x01")?;
    let (kind, entry) = next_box(&mut entries)?;
    if !entries.is_empty()
        || kind != if video { *b"avc1" } else { *b"mp4a" }
        || entry.get(0..8)? != b"\0\0\0\0\0\0\0\x01"
    {
        return None;
    }
    let id = versioned(tkhd?, 12, 20)?;
    let timescale = versioned(mdhd?, 12, 20)?;
    (id != 0 && timescale != 0).then_some((
        video,
        Track {
            id,
            timescale,
            defaults: Defaults::default(),
            end: 0,
            frames: 0,
            refs: Vec::new(),
        },
    ))
}

fn movie(mut bytes: &[u8]) -> Option<(Track, Option<Track>)> {
    let (mut video, mut audio, mut defaults) = (None, None, None);
    while !bytes.is_empty() {
        let (kind, value) = next_box(&mut bytes)?;
        let previous = match &kind {
            b"trak" => {
                let (is_video, track) = track(value)?;
                if is_video {
                    video.replace(track)
                } else {
                    audio.replace(track)
                }
            }
            b"mvex" if defaults.replace(value).is_none() => continue,
            b"mvhd" | b"udta" => continue,
            _ => return None,
        };
        if previous.is_some() {
            return None;
        }
    }
    let mut video = video?;
    let mut defaults = defaults?;
    let mut seen = [false; 2];
    while !defaults.is_empty() {
        let (kind, value) = next_box(&mut defaults)?;
        if kind != *b"trex" || value.len() != 24 || number(value, 0)? != 0 || number(value, 8)? != 1
        {
            return None;
        }
        let id = number(value, 4)?;
        let (track, seen) = if id == video.id {
            (&mut video, &mut seen[0])
        } else {
            (audio.as_mut().filter(|track| track.id == id)?, &mut seen[1])
        };
        if std::mem::replace(seen, true) {
            return None;
        }
        track.defaults = Defaults {
            duration: number(value, 12)?,
            size: number(value, 16)?,
            flags: number(value, 20)?,
        };
    }
    (seen == [true, audio.is_some()]).then_some((video, audio))
}

fn take_number(bytes: &mut &[u8]) -> Option<u32> {
    let (value, rest) = bytes.split_first_chunk::<4>()?;
    *bytes = rest;
    Some(u32::from_be_bytes(*value))
}

fn run(
    tfhd: &[u8],
    tfdt: &[u8],
    mut trun: &[u8],
    track: &mut Track,
    step: Option<u32>,
    media: std::ops::Range<u64>,
    reference_size: u32,
) -> Option<std::ops::Range<u64>> {
    let flags = number(tfhd, 0)?;
    if flags & !0x02003a != 0 || flags & 0x020000 == 0 {
        return None;
    }
    let mut optional = tfhd.get(8..)?;
    if flags & 2 != 0 && take_number(&mut optional)? != 1 {
        return None;
    }
    let mut defaults = track.defaults;
    for (bit, field) in [
        (8, &mut defaults.duration),
        (16, &mut defaults.size),
        (32, &mut defaults.flags),
    ] {
        if flags & bit != 0 {
            *field = take_number(&mut optional)?;
        }
    }
    let start = match tfdt {
        [0, 0, 0, 0, pts @ ..] => u64::from(u32::from_be_bytes(pts.try_into().ok()?)),
        [1, 0, 0, 0, pts @ ..] => u64::from_be_bytes(pts.try_into().ok()?),
        _ => return None,
    };
    if !optional.is_empty() || start != track.end {
        return None;
    }
    let flags = take_number(&mut trun)?;
    let count = take_number(&mut trun)?;
    if flags & !0x00000f05 != 0 || flags & 1 == 0 || flags & 0x404 == 0x404 || count == 0 {
        return None;
    }
    let offset = u64::from(take_number(&mut trun)?);
    let first_flags = if flags & 4 != 0 {
        Some(take_number(&mut trun)?)
    } else {
        None
    };
    let stride = (flags & 0xf00).count_ones() as usize * 4;
    if trun.len() != stride.checked_mul(count as usize)? {
        return None;
    }
    let mut duration = 0_u64;
    let mut length = 0_u64;
    if stride == 0 {
        if defaults.duration == 0
            || defaults.size == 0
            || step.is_some_and(|s| s != defaults.duration)
            || !sync_sample(first_flags.unwrap_or(defaults.flags))
        {
            return None;
        }
        duration = u64::from(count) * u64::from(defaults.duration);
        length = u64::from(count) * u64::from(defaults.size);
    } else {
        for sample in 0..count {
            let delta = if flags & 0x100 != 0 {
                take_number(&mut trun)?
            } else {
                defaults.duration
            };
            let size = if flags & 0x200 != 0 {
                take_number(&mut trun)?
            } else {
                defaults.size
            };
            let sample_flags = if flags & 0x400 != 0 {
                take_number(&mut trun)?
            } else {
                first_flags.unwrap_or(defaults.flags)
            };
            if delta == 0
                || size == 0
                || step.is_some_and(|s| s != delta)
                || (sample == 0 && !sync_sample(sample_flags))
                || (flags & 0x800 != 0 && take_number(&mut trun)? != 0)
            {
                return None;
            }
            duration += u64::from(delta);
            length += u64::from(size);
        }
    }
    let end = offset.checked_add(length)?;
    if offset < media.start || end > media.end {
        return None;
    }
    track.end = track.end.checked_add(duration)?;
    track.frames = track.frames.checked_add(u64::from(count))?;
    track
        .refs
        .push((reference_size, u32::try_from(duration).ok()?));
    Some(offset..end)
}

fn sync_sample(flags: u32) -> bool {
    flags & 0x10000 == 0 && (flags >> 24) & 3 != 1
}

fn sidx_size(track: &Track) -> u32 {
    40 + 12 * track.refs.len() as u32
}

fn write_sidx(prefix: &mut Vec<u8>, track: &Track, first_offset: u64) {
    prefix.extend(sidx_size(track).to_be_bytes());
    prefix.extend(b"sidx\x01\0\0\0");
    prefix.extend(track.id.to_be_bytes());
    prefix.extend(track.timescale.to_be_bytes());
    prefix.extend(0_u64.to_be_bytes()); // earliest presentation time in the source track clock
    prefix.extend(first_offset.to_be_bytes());
    prefix.extend(0_u16.to_be_bytes());
    prefix.extend((track.refs.len() as u16).to_be_bytes());
    for (size, duration) in &track.refs {
        prefix.extend(size.to_be_bytes());
        prefix.extend(duration.to_be_bytes());
        prefix.extend(0x90000000_u32.to_be_bytes()); // starts with a type-1 SAP
    }
}

struct Reader<R> {
    source: R,
    bytes_left: usize,
    started: Instant,
    length: u64,
}

impl<R: Read + Seek> Reader<R> {
    fn read(&mut self, offset: u64, bytes: &mut [u8]) -> Option<()> {
        self.bytes_left = self.bytes_left.checked_sub(bytes.len())?;
        if self.started.elapsed() >= MAX_PREPARATION
            || offset.checked_add(bytes.len() as u64)? > self.length
        {
            return None;
        }
        self.source.seek(SeekFrom::Start(offset)).ok()?;
        self.source.read_exact(bytes).ok()
    }

    fn header(&mut self, offset: u64) -> Option<Header> {
        let mut bytes = [0; 16];
        self.read(offset, &mut bytes[..8])?;
        if number(&bytes, 0)? == 1 {
            self.read(offset + 8, &mut bytes[8..])?;
        }
        let item = header(&bytes)?;
        (offset.checked_add(item.size)? <= self.length).then_some(item)
    }
}

pub(crate) fn prepare(
    source: impl Read + Seek,
    length: u64,
    timeline: &MediaTimelineV2,
) -> Option<ReplayIndex> {
    let mut reader = Reader {
        source,
        length,
        bytes_left: MAX_READ_BYTES,
        started: Instant::now(),
    };
    let ftyp = reader.header(0)?;
    let moov = reader.header(ftyp.size)?;
    let source_start = ftyp.size + moov.size;
    if ftyp.kind != *b"ftyp" || moov.kind != *b"moov" || source_start > MAX_BOX_BYTES as u64 {
        return None;
    }
    let mut prefix = vec![0; source_start as usize];
    reader.read(0, &mut prefix)?;
    let (mut video, mut audio) = movie(&prefix[ftyp.size as usize + moov.header..])?;
    let vt = &timeline.video;
    let expected_end = u64::try_from(vt.one_past_last_pts.get()).ok()?;
    let frame_count = vt.frame_count.get();
    let step = u32::try_from(expected_end.checked_div(frame_count)?).ok()?;
    if timeline.video.codec != "h264"
        || !vt.exact_cfr
        || vt.first_pts.get() != 0
        || vt.time_base.numerator() != 1
        || vt.time_base.denominator() != u64::from(video.timescale)
        || u64::from(step) * frame_count != expected_end
        || timeline.audio.present != audio.is_some()
    {
        return None;
    }
    let audio_end = if let Some(track) = &audio {
        let at = &timeline.audio;
        let base = at.time_base?;
        if at.codec.as_deref() != Some("aac")
            || at.first_pts?.get() != 0
            || at.replay_start?.get() != 0
            || base.numerator() != 1
            || base.denominator() != u64::from(track.timescale)
        {
            return None;
        }
        let end = u64::try_from(at.replay_end?.get())
            .ok()?
            .checked_mul(u64::from(track.timescale))?;
        if end % REPLAY_TICKS_PER_SECOND != 0 {
            return None;
        }
        end / REPLAY_TICKS_PER_SECOND
    } else {
        0
    };

    let mut offset = source_start;
    let mut scratch = Vec::new();
    while offset < length {
        let fragment = reader.header(offset)?;
        if fragment.kind == *b"mfra" && offset + fragment.size == length && fragment.size >= 24 {
            let mut footer = [0; 16];
            reader.read(length - 16, &mut footer)?;
            if footer[..12] != *b"\0\0\0\x10mfro\0\0\0\0"
                || u64::from(number(&footer, 12)?) != fragment.size
            {
                return None;
            }
            break;
        }
        if fragment.kind != *b"moof"
            || fragment.size > MAX_BOX_BYTES as u64
            || video.refs.len() == MAX_REFERENCES
        {
            return None;
        }
        let data = reader.header(offset + fragment.size)?;
        let size = fragment.size.checked_add(data.size)?;
        if data.kind != *b"mdat" || size >= (1 << 31) || data.size <= data.header as u64 {
            return None;
        }
        scratch.resize(fragment.size as usize - fragment.header, 0);
        reader.read(offset + fragment.header as u64, &mut scratch)?;
        let mut remaining = scratch.as_slice();
        let mut spans = [None, None];
        let mut sequence_seen = false;
        let fragment_number = video.refs.len();
        while !remaining.is_empty() {
            let (kind, bytes) = next_box(&mut remaining)?;
            if kind == *b"mfhd" && !sequence_seen && bytes.len() == 8 && number(bytes, 0)? == 0 {
                sequence_seen = true;
                continue;
            }
            if kind != *b"traf" {
                return None;
            }
            let [tfhd, tfdt, trun] = fields(bytes, [*b"tfhd", *b"tfdt", *b"trun"])?;
            let tfhd = tfhd?;
            let id = number(tfhd, 4)?;
            let (track, slot, step) = if video.id == id {
                (&mut video, 0, Some(step))
            } else {
                (audio.as_mut().filter(|track| track.id == id)?, 1, None)
            };
            if spans[slot].is_some() || (slot == 1 && track.refs.len() != fragment_number) {
                return None;
            }
            spans[slot] = Some(run(
                tfhd,
                tfdt?,
                trun?,
                track,
                step,
                fragment.size + data.header as u64..size,
                size as u32,
            )?);
        }
        let [Some(picture), sound] = spans else {
            return None;
        };
        if !sequence_seen
            || sound.is_some_and(|sound| picture.start < sound.end && sound.start < picture.end)
        {
            return None;
        }
        offset += size;
    }
    if video.refs.is_empty()
        || video.end != expected_end
        || video.frames != frame_count
        || audio
            .as_ref()
            .is_some_and(|track| track.refs.is_empty() || track.end != audio_end)
    {
        return None;
    }
    if let Some(audio) = &audio {
        write_sidx(&mut prefix, audio, u64::from(sidx_size(&video)));
    }
    write_sidx(&mut prefix, &video, 0);
    Some(ReplayIndex {
        length: (prefix.len() as u64).checked_add(offset - source_start)?,
        prefix: prefix.into(),
        source_start,
    })
}
