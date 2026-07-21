use super::error::StorageError;

pub(super) const MAX_FRAME_PAYLOAD: u32 = 128 * 1024 * 1024;

/// Write a length-prefixed, CRC32C-checksummed frame into `buf`
pub(super) fn write_frame<
    T: wincode::SchemaWrite<wincode::config::DefaultConfig, Src = T>,
>(
    buf: &mut Vec<u8>,
    value: &T,
) {
    let size = wincode::serialized_size(value).expect("serialized_size cannot fail");
    assert!(
        size <= MAX_FRAME_PAYLOAD as u64,
        "frame exceeds maximum payload size ({size} > {MAX_FRAME_PAYLOAD})"
    );

    #[allow(clippy::cast_possible_truncation)]
    buf.reserve(size as usize + 8);
    let start = buf.len();
    buf.extend_from_slice(
        &(u32::try_from(size).expect("size fits in u32; qed")).to_le_bytes(),
    );
    wincode::serialize_into(&mut *buf, value).expect("serialize_into cannot fail");
    let crc = crc_fast::crc32_iscsi(&buf[start..]);
    buf.extend_from_slice(&crc.to_le_bytes());
}

/// Serialize a value into a self-contained frame buffer
pub(super) fn serialize_frame<
    T: wincode::SchemaWrite<wincode::config::DefaultConfig, Src = T>,
>(
    value: &T,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_frame(&mut buf, value);
    buf
}

/// Read a frame at `offset`. Returns the payload slice and total frame size
pub(super) fn read_frame(
    data: &[u8],
    offset: usize,
) -> Result<Option<(&[u8], usize)>, StorageError> {
    let remaining = data.len().saturating_sub(offset);
    if remaining < 4 {
        return Ok(None);
    }

    let end = offset.checked_add(4).ok_or(StorageError::MathError)?;
    let len = u32::from_le_bytes(data[offset..end].try_into().unwrap());
    if len > MAX_FRAME_PAYLOAD {
        return Ok(None);
    }

    let len_usize = len as usize;
    let frame_size = 4usize
        .checked_add(len_usize)
        .and_then(|v| v.checked_add(4))
        .ok_or(StorageError::MathError)?;
    if remaining < frame_size {
        return Ok(None);
    }

    let crc_offset = offset
        .checked_add(4)
        .and_then(|v| v.checked_add(len_usize))
        .ok_or(StorageError::MathError)?;
    let crc_end = crc_offset.checked_add(4).ok_or(StorageError::MathError)?;
    let stored_crc = u32::from_le_bytes(data[crc_offset..crc_end].try_into().unwrap());
    let computed_crc = crc_fast::crc32_iscsi(&data[offset..crc_offset]);

    if stored_crc != computed_crc {
        if offset
            .checked_add(frame_size)
            .ok_or(StorageError::MathError)?
            >= data.len()
        {
            return Ok(None); // tail truncation
        }
        return Err(StorageError::CrcMismatch {
            offset: offset as u64,
            expected: stored_crc,
            actual: computed_crc,
        });
    }

    let payload_start = offset.checked_add(4).ok_or(StorageError::MathError)?;
    Ok(Some((&data[payload_start..crc_offset], frame_size)))
}

/// Deserialize a single frame file (e.g., header.bin, checkpoint.meta)
pub(super) fn read_frame_file<T>(
    path: &std::path::Path,
) -> Result<Option<T>, StorageError>
where
    T: for<'a> wincode::SchemaRead<'a, wincode::config::DefaultConfig, Dst = T>,
{
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(StorageError::Io(e)),
    };

    let (payload, _) = match read_frame(&data, 0) {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(StorageError::CrcMismatch { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    match wincode::deserialize(payload) {
        Ok(v) => Ok(Some(v)),
        Err(_) => Err(StorageError::SerdeFailed {
            path: path.display().to_string(),
        }),
    }
}
