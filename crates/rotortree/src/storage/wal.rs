use crate::Hash;

use super::{
    error::StorageError,
    frame,
};

pub(crate) const FILE_MAGIC: [u8; 4] = [0x52, 0x4F, 0x54, 0x52];

#[derive(Debug, wincode::SchemaWrite, wincode::SchemaRead)]
pub(crate) enum WalHeader {
    V1 {
        magic: [u8; 4],
        n: u32,
        max_depth: u32,
    },
}

/// new cow moo
#[derive(Debug, wincode::SchemaWrite, wincode::SchemaRead)]
pub(crate) enum NewCow<'a> {
    Owned(Vec<Hash>),
    Borrowed(&'a [Hash]),
}

impl NewCow<'_> {
    pub fn as_slice(&self) -> &[Hash] {
        match self {
            NewCow::Owned(v) => v,
            NewCow::Borrowed(s) => s,
        }
    }
}

impl PartialEq for NewCow<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for NewCow<'_> {}

#[derive(Debug, PartialEq, Eq, wincode::SchemaWrite, wincode::SchemaRead)]
pub(crate) enum WalEntry<'a> {
    V1(WalEntryV1<'a>),
}

#[derive(Debug, PartialEq, Eq, wincode::SchemaWrite, wincode::SchemaRead)]
pub(crate) struct WalEntryV1<'a> {
    pub seq: u64,
    pub payload: WalPayload<'a>,
}

#[derive(Debug, PartialEq, Eq, wincode::SchemaWrite, wincode::SchemaRead)]
pub(crate) enum WalPayload<'a> {
    Single(Hash),
    Batch(NewCow<'a>),
}

pub(crate) fn serialize_header(n: u32, max_depth: u32) -> Vec<u8> {
    let header = WalHeader::V1 {
        magic: FILE_MAGIC,
        n,
        max_depth,
    };
    frame::serialize_frame(&header)
}

/// Returns (n, max_depth, header_frame_size).
pub(crate) fn deserialize_header(data: &[u8]) -> Result<(u32, u32, usize), StorageError> {
    let (payload, frame_size) =
        frame::read_frame(data, 0)?.ok_or(StorageError::WalCorrupted { offset: 0 })?;
    let header: WalHeader = wincode::deserialize(payload)
        .map_err(|_| StorageError::WalCorrupted { offset: 0 })?;
    let WalHeader::V1 {
        magic,
        n,
        max_depth,
    } = header;
    if magic != FILE_MAGIC {
        return Err(StorageError::WalCorrupted { offset: 0 });
    }
    Ok((n, max_depth, frame_size))
}

pub(crate) fn serialize_entry(buf: &mut Vec<u8>, seq: u64, payload: WalPayload<'_>) {
    let entry = WalEntry::V1(WalEntryV1 { seq, payload });
    frame::write_frame(buf, &entry);
}

pub(crate) fn deserialize_entry<'a>(
    data: &'a [u8],
    offset: usize,
    last_seq: Option<u64>,
) -> Result<Option<(WalEntryV1<'a>, usize)>, StorageError> {
    let (payload, frame_size) = match frame::read_frame(data, offset)? {
        Some(r) => r,
        None => return Ok(None),
    };

    let entry: WalEntry<'a> = match wincode::deserialize(payload) {
        Ok(e) => e,
        Err(_) => {
            return Err(StorageError::WalCorrupted {
                offset: offset as u64,
            });
        }
    };

    let WalEntry::V1(v1) = entry;

    if let Some(last) = last_seq
        && v1.seq != last.checked_add(1).ok_or(StorageError::MathError)?
    {
        return Ok(None);
    }

    Ok(Some((v1, frame_size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let buf = serialize_header(4, 32);
        let (n, max_depth, consumed) = deserialize_header(&buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(max_depth, 32);
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn header_bad_magic() {
        let mut buf = serialize_header(2, 32);
        buf[4] ^= 0xFF;
        assert!(deserialize_header(&buf).is_err());
    }

    #[test]
    fn header_bad_crc() {
        let mut buf = serialize_header(2, 32);
        let last = buf.len() - 1;
        buf[last] ^= 0xFF;
        assert!(deserialize_header(&buf).is_err());
    }

    #[test]
    fn single_entry_round_trip() {
        let leaf = [42u8; 32];
        let mut buf = Vec::new();
        serialize_entry(&mut buf, 0, WalPayload::Single(leaf));

        let (entry, consumed) = deserialize_entry(&buf, 0, None).unwrap().unwrap();
        assert_eq!(consumed, buf.len());
        assert_eq!(entry.seq, 0);
        assert_eq!(entry.payload, WalPayload::Single(leaf));
    }

    #[test]
    fn batch_entry_round_trip() {
        let leaves: Vec<Hash> = (0..10u8)
            .map(|i| {
                let mut h = [0u8; 32];
                h[0] = i;
                h
            })
            .collect();
        let mut buf = Vec::new();
        serialize_entry(&mut buf, 5, WalPayload::Batch(NewCow::Borrowed(&leaves)));

        let (entry, consumed) = deserialize_entry(&buf, 0, Some(4)).unwrap().unwrap();
        assert_eq!(consumed, buf.len());
        assert_eq!(entry.seq, 5);
        assert_eq!(entry.payload, WalPayload::Batch(NewCow::Borrowed(&leaves)));
    }

    #[test]
    fn crc_corruption_mid_file() {
        let leaf = [1u8; 32];
        let mut buf = Vec::new();
        serialize_entry(&mut buf, 0, WalPayload::Single(leaf));
        buf[6] ^= 0xFF;
        // Append extra bytes so this is not at the tail.
        buf.extend_from_slice(&[0u8; 100]);
        let result = deserialize_entry(&buf, 0, None);
        assert!(matches!(result, Err(StorageError::CrcMismatch { .. })));
    }

    #[test]
    fn truncated_tail_returns_none() {
        let mut buf = Vec::new();
        serialize_entry(&mut buf, 0, WalPayload::Single([1u8; 32]));
        buf.pop();
        assert!(deserialize_entry(&buf, 0, None).unwrap().is_none());
    }

    #[test]
    fn seq_gap_returns_none() {
        let mut buf = Vec::new();
        serialize_entry(&mut buf, 5, WalPayload::Single([1u8; 32]));
        assert!(deserialize_entry(&buf, 0, Some(3)).unwrap().is_none());
    }

    #[test]
    fn multiple_entries_sequential() {
        let mut buf = Vec::new();
        for i in 0..5u64 {
            let mut leaf = [0u8; 32];
            leaf[0] = i as u8;
            serialize_entry(&mut buf, i, WalPayload::Single(leaf));
        }

        let mut offset = 0;
        let mut last_seq = None;
        let mut count = 0u64;

        while let Some((entry, consumed)) =
            deserialize_entry(&buf, offset, last_seq).unwrap()
        {
            assert_eq!(entry.seq, count);
            last_seq = Some(entry.seq);
            offset += consumed;
            count += 1;
        }

        assert_eq!(count, 5);
        assert_eq!(offset, buf.len());
    }
}
