use super::{Corpus, Folding, TagInfo};
use crate::kinds::{Media, TagKind};
use crate::search::Rating;
use std::collections::HashMap;
use std::io::{self, Read, Write};

const MAGIC: &[u8; 8] = b"FYINDX02";
const NONE: i64 = i64::MIN;
const NO_KIND: u8 = u8::MAX;
const MEDIA: [Media; 3] = [Media::Image, Media::Gif, Media::Video];

pub trait Plain: Sized + Copy {
    const SIZE: usize;
    fn put(self, out: &mut Vec<u8>);
    fn take(bytes: &[u8]) -> Self;
}

macro_rules! plain {
    ($($kind:ty),*) => {$(
        impl Plain for $kind {
            const SIZE: usize = std::mem::size_of::<$kind>();
            fn put(self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn take(bytes: &[u8]) -> Self {
                <$kind>::from_le_bytes(bytes.try_into().unwrap_or_default())
            }
        }
    )*};
}

plain!(u8, i8, u32, u64, i64, u128, f32, f64);

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("damaged index: {what}"))
}

pub fn write_values<T: Plain>(out: &mut impl Write, values: &[T]) -> io::Result<()> {
    out.write_all(&(values.len() as u64).to_le_bytes())?;
    let mut buffer = Vec::with_capacity(1 << 20);
    for chunk in values.chunks((1 << 20) / T::SIZE.max(1)) {
        buffer.clear();
        for value in chunk {
            value.put(&mut buffer);
        }
        out.write_all(&buffer)?;
    }
    Ok(())
}

pub fn read_values<T: Plain>(input: &mut impl Read) -> io::Result<Vec<T>> {
    let length = read_one::<u64>(input)? as usize;
    let mut bytes = vec![0; length * T::SIZE];
    input.read_exact(&mut bytes)?;
    Ok(bytes.chunks_exact(T::SIZE).map(T::take).collect())
}

pub fn read_one<T: Plain>(input: &mut impl Read) -> io::Result<T> {
    let mut bytes = vec![0; T::SIZE];
    input.read_exact(&mut bytes)?;
    Ok(T::take(&bytes))
}

pub fn write_text(out: &mut impl Write, text: &str) -> io::Result<()> {
    out.write_all(&(text.len() as u32).to_le_bytes())?;
    out.write_all(text.as_bytes())
}

pub fn read_text(input: &mut impl Read) -> io::Result<String> {
    let mut bytes = vec![0; read_one::<u32>(input)? as usize];
    input.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| invalid("text"))
}

fn optional(value: Option<i64>) -> i64 {
    value.unwrap_or(NONE)
}

fn present(value: i64) -> Option<i64> {
    (value != NONE).then_some(value)
}

fn as_u64(values: &[usize]) -> Vec<u64> {
    values.iter().map(|value| *value as u64).collect()
}

fn as_usize(values: Vec<u64>) -> Vec<usize> {
    values.into_iter().map(|value| value as usize).collect()
}

impl Corpus {
    pub fn save(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(MAGIC)?;
        out.write_all(&self.site_total.to_le_bytes())?;
        out.write_all(&(self.tags.len() as u64).to_le_bytes())?;
        for (id, info) in &self.tags {
            out.write_all(&id.to_le_bytes())?;
            write_text(out, &info.name)?;
            out.write_all(&[info.kind.map_or(NO_KIND, |kind| kind.code() as u8)])?;
            out.write_all(&optional(info.count).to_le_bytes())?;
        }
        write_values(out, &self.ids)?;
        write_values(
            out,
            &self
                .ratings
                .iter()
                .map(|rating| *rating as u8)
                .collect::<Vec<u8>>(),
        )?;
        write_values(out, &self.scores)?;
        write_values(out, &self.score_sites)?;
        write_values(out, &self.score_percentiles)?;
        write_values(
            out,
            &self
                .posted_at
                .iter()
                .map(|at| optional(*at))
                .collect::<Vec<i64>>(),
        )?;
        write_values(out, &self.md5s)?;
        write_values(
            out,
            &self
                .media
                .iter()
                .map(|media| MEDIA.iter().position(|known| known == media).unwrap_or(0) as u8)
                .collect::<Vec<u8>>(),
        )?;
        write_values(out, &as_u64(&self.indptr))?;
        write_values(out, &self.cols)?;
        write_values(out, &self.row_scale)?;
        write_values(out, &self.learned)?;
        write_values(out, &self.learned_scale)?;
        write_values(out, &self.tag_of_col)?;
        write_values(
            out,
            &self
                .artist_col
                .iter()
                .map(|artist| *artist as u8)
                .collect::<Vec<u8>>(),
        )?;
        write_values(out, &self.idf_of_col)?;
        write_values(out, &as_u64(&self.posts_of_col))?;
        write_values(
            out,
            &self
                .learned_row_of_col
                .iter()
                .map(|row| optional(row.map(|row| row as i64)))
                .collect::<Vec<i64>>(),
        )?;
        out.write_all(&(self.folding.len() as u64).to_le_bytes())?;
        for (col, folding) in &self.folding {
            out.write_all(&col.to_le_bytes())?;
            out.write_all(&(folding.posts as u64).to_le_bytes())?;
            write_values(out, &folding.sum)?;
        }
        Ok(())
    }

    pub fn load(input: &mut impl Read) -> io::Result<Corpus> {
        let mut magic = [0; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(invalid("not an index"));
        }
        let site_total = read_one::<f64>(input)?;
        let tag_count = read_one::<u64>(input)? as usize;
        let mut tags = HashMap::with_capacity(tag_count);
        for _ in 0..tag_count {
            let id = read_one::<i64>(input)?;
            let name = read_text(input)?;
            let kind = read_one::<u8>(input)?;
            let count = present(read_one::<i64>(input)?);
            tags.insert(
                id,
                TagInfo {
                    name,
                    kind: (kind != NO_KIND)
                        .then(|| TagKind::from_code(kind as i64))
                        .flatten(),
                    count,
                },
            );
        }
        let ids: Vec<i64> = read_values(input)?;
        let ratings = read_values::<u8>(input)?
            .into_iter()
            .map(|code| {
                Rating::ALL
                    .get(code as usize)
                    .copied()
                    .ok_or_else(|| invalid("rating"))
            })
            .collect::<io::Result<_>>()?;
        let scores = read_values(input)?;
        let score_sites = read_values(input)?;
        let score_percentiles = read_values(input)?;
        let posted_at = read_values::<i64>(input)?
            .into_iter()
            .map(present)
            .collect();
        let md5s = read_values(input)?;
        let media = read_values::<u8>(input)?
            .into_iter()
            .map(|code| {
                MEDIA
                    .get(code as usize)
                    .copied()
                    .ok_or_else(|| invalid("media"))
            })
            .collect::<io::Result<_>>()?;
        let indptr = as_usize(read_values(input)?);
        let cols = read_values(input)?;
        let row_scale = read_values(input)?;
        let learned = read_values(input)?;
        let learned_scale = read_values(input)?;
        let tag_of_col: Vec<i64> = read_values(input)?;
        let artist_col = read_values::<u8>(input)?
            .into_iter()
            .map(|artist| artist != 0)
            .collect();
        let idf_of_col = read_values(input)?;
        let posts_of_col = as_usize(read_values(input)?);
        let learned_row_of_col = read_values::<i64>(input)?
            .into_iter()
            .map(|row| present(row).map(|row| row as usize))
            .collect();
        let folding_count = read_one::<u64>(input)? as usize;
        let mut folding = HashMap::with_capacity(folding_count);
        for _ in 0..folding_count {
            let col = read_one::<u32>(input)?;
            let posts = read_one::<u64>(input)? as usize;
            folding.insert(
                col,
                Folding {
                    sum: read_values(input)?,
                    posts,
                },
            );
        }
        Ok(Corpus {
            site_total,
            tag_ids: tags
                .iter()
                .map(|(id, info)| (info.name.clone(), *id))
                .collect(),
            tags,
            rows: ids.iter().enumerate().map(|(row, id)| (*id, row)).collect(),
            col_of_tag: tag_of_col
                .iter()
                .enumerate()
                .map(|(col, tag)| (*tag, col as u32))
                .collect(),
            ids,
            ratings,
            scores,
            score_sites,
            score_percentiles,
            posted_at,
            md5s,
            media,
            indptr,
            cols,
            row_scale,
            learned,
            learned_scale,
            tag_of_col,
            artist_col,
            idf_of_col,
            posts_of_col,
            learned_row_of_col,
            folding,
            postings: std::sync::OnceLock::new(),
        })
    }
}
