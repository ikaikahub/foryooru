use std::collections::HashMap;
use std::sync::OnceLock;

const BUNDLED: &[u8] = include_bytes!("../../assets/tag_vectors.bin");
const MAGIC: &[u8] = b"FYTV";
const QUANTIZE_SCALE: f32 = 127.0;

pub struct TagVectors {
    pub dimensions: usize,
    index: HashMap<String, usize>,
    values: Vec<f32>,
}

impl TagVectors {
    pub fn row(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    pub fn values(&self, row: usize) -> &[f32] {
        let start = row * self.dimensions;
        &self.values[start..start + self.dimensions]
    }
}

pub fn learned_key(dimension: usize) -> i64 {
    -1 - dimension as i64
}

pub fn dimension_of(key: i64) -> Option<usize> {
    (key < 0).then(|| (-1 - key) as usize)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.at..self.at + count)?;
        self.at += count;
        Some(slice)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }
}

fn parse(bytes: &[u8]) -> Option<TagVectors> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.take(MAGIC.len())? != MAGIC {
        return None;
    }
    let count = reader.u32()?;
    let dimensions = reader.u32()?;
    let mut index = HashMap::with_capacity(count);
    for row in 0..count {
        let length = reader.u16()? as usize;
        let name = std::str::from_utf8(reader.take(length)?).ok()?;
        index.insert(name.to_owned(), row);
    }
    let values = reader
        .take(count * dimensions)?
        .iter()
        .map(|byte| *byte as i8 as f32 / QUANTIZE_SCALE)
        .collect();
    Some(TagVectors {
        dimensions,
        index,
        values,
    })
}

pub fn bundled() -> &'static TagVectors {
    static VECTORS: OnceLock<TagVectors> = OnceLock::new();
    VECTORS.get_or_init(|| parse(BUNDLED).expect("the bundled tag vectors are malformed"))
}
