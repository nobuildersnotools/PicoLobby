use crate::binary_reader::ReadBytes;
use crate::prelude::{BinaryReader, BinaryReaderError, Prefixed};
use std::io::Read;
use tracing::warn;

pub trait ReadLengthPrefix: Sized + ReadBytes {
    fn read_to_usize(reader: &mut BinaryReader) -> Result<usize, BinaryReaderError>;
}

impl<L> ReadBytes for Prefixed<L, String>
where
    L: ReadLengthPrefix,
{
    #[inline]
    fn read(reader: &mut BinaryReader) -> Result<Self, BinaryReaderError> {
        let length = L::read_to_usize(reader)?;
        // A string of `length` bytes can never exceed the bytes still left in the
        // packet buffer. Reject an oversized prefix *before* allocating so a tiny
        // packet that claims a multi-gigabyte length cannot exhaust memory.
        if length > reader.remaining() {
            return Err(BinaryReaderError::UnexpectedEof);
        }
        let mut bytes = vec![0u8; length];
        reader.0.read_exact(&mut bytes)?;
        Ok(Prefixed::new(String::from_utf8(bytes).unwrap_or_else(
            |_| {
                warn!(
                    "Invalid string of length {} ended at index {}",
                    length,
                    reader.position()
                );
                create_repeated_string(length, '�')
            },
        )))
    }
}

fn create_repeated_string(length: usize, ch: char) -> String {
    std::iter::repeat_n(ch, length).collect()
}

impl<L, T> ReadBytes for Prefixed<L, Vec<T>>
where
    L: ReadLengthPrefix,
    T: ReadBytes,
{
    #[inline]
    fn read(reader: &mut BinaryReader) -> Result<Self, BinaryReaderError> {
        let length = L::read_to_usize(reader)?;
        // Every element occupies at least one byte on the wire, so a valid element
        // count can never exceed the remaining bytes. Bounding the pre-allocation
        // here stops a small packet from forcing a huge upfront allocation.
        if length > reader.remaining() {
            return Err(BinaryReaderError::UnexpectedEof);
        }
        let mut vec = Vec::with_capacity(length);
        for _ in 0..length {
            vec.push(reader.read()?);
        }
        Ok(Prefixed::new(vec))
    }
}

pub(crate) fn from_i32(len: i32) -> Result<usize, BinaryReaderError> {
    len.try_into().map_err(|_| {
        BinaryReaderError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid length: negative or too large for usize",
        ))
    })
}

impl ReadLengthPrefix for i32 {
    fn read_to_usize(reader: &mut BinaryReader) -> Result<usize, BinaryReaderError> {
        let len = reader.read()?;
        from_i32(len)
    }
}

impl ReadLengthPrefix for u16 {
    fn read_to_usize(reader: &mut BinaryReader) -> Result<usize, BinaryReaderError> {
        Ok(reader.read::<u16>()?.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::{BinaryReader, BinaryReaderError, VarIntPrefixed, VarIntPrefixedString};

    #[test]
    fn string_prefix_larger_than_remaining_is_rejected_without_allocating() {
        // VarInt 0xFF 0xFF 0xFF 0xFF 0x07 == i32::MAX, followed by no payload.
        // Before the bound this allocated ~2 GiB; now it must fail fast.
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0x07];
        let mut reader = BinaryReader::new(&data);
        let result = reader.read::<VarIntPrefixedString>();
        assert!(matches!(result, Err(BinaryReaderError::UnexpectedEof)));
    }

    #[test]
    fn vec_prefix_larger_than_remaining_is_rejected_without_allocating() {
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0x07];
        let mut reader = BinaryReader::new(&data);
        let result = reader.read::<VarIntPrefixed<Vec<u8>>>();
        assert!(matches!(result, Err(BinaryReaderError::UnexpectedEof)));
    }

    #[test]
    fn well_formed_string_still_decodes() {
        // VarInt 5, then "HELLO".
        let data = [0x05, 72, 69, 76, 76, 79];
        let mut reader = BinaryReader::new(&data);
        let parsed = reader.read::<VarIntPrefixedString>().unwrap();
        assert_eq!(parsed.into_inner(), "HELLO");
    }
}
