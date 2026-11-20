use crate::error::{Error, Result};
use foryooru_core::search::Rating;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

const MAGIC: &[u8; 8] = b"FYSNAP01";
const NO_KIND: u8 = u8::MAX;
const NO_DATE: i64 = i64::MIN;

pub struct SnapshotTag {
    pub id: i64,
    pub name: String,
    pub kind: Option<u8>,
    pub count: i64,
}

pub struct SnapshotPost {
    pub id: i64,
    pub rating: Rating,
    pub score: i64,
    pub posted_at: Option<i64>,
    pub md5: String,
    pub ext: String,
    pub width: u32,
    pub height: u32,
    pub tags: Vec<i64>,
}

fn kind_code(kind: &str) -> Option<u8> {
    match kind {
        "tag" => Some(0),
        "artist" => Some(1),
        "copyright" => Some(3),
        "character" => Some(4),
        "metadata" => Some(5),
        "deprecated" => Some(6),
        _ => None,
    }
}

fn rating_of(lewdness: &str) -> Option<Rating> {
    match lewdness {
        "0" => Some(Rating::General),
        "1" => Some(Rating::Sensitive),
        "2" => Some(Rating::Questionable),
        "3" => Some(Rating::Explicit),
        _ => None,
    }
}

fn rating_code(rating: Rating) -> u8 {
    rating as u8
}

fn rating_from_code(code: u8) -> Result<Rating> {
    Rating::ALL
        .get(code as usize)
        .copied()
        .ok_or(Error::Corrupt("rating"))
}

fn copy_rows(
    container: &str,
    sql: &str,
    mut each: impl FnMut(&[&str]) -> Result<()>,
) -> Result<()> {
    let mut child = Command::new("docker")
        .args([
            "exec", container, "psql", "-U", "nice", "-d", "kimochii", "-At", "-c",
        ])
        .arg(format!("COPY ({sql}) TO STDOUT"))
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or(Error::Corrupt("psql output"))?;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let fields: Vec<&str> = line.split('\t').collect();
        each(&fields)?;
    }
    if !child.wait()?.success() {
        return Err(Error::Export);
    }
    Ok(())
}

fn field<'a>(fields: &[&'a str], index: usize) -> Result<&'a str> {
    fields
        .get(index)
        .copied()
        .ok_or(Error::Corrupt("missing column"))
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T> {
    text.parse().map_err(|_| Error::Corrupt("number"))
}

fn tag_array(text: &str) -> Result<Vec<i64>> {
    let inner = text.trim_start_matches('{').trim_end_matches('}');
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner.split(',').map(number).collect()
}

struct Output<W: Write>(W);

impl<W: Write> Output<W> {
    fn bytes(&mut self, value: &[u8]) -> Result<()> {
        Ok(self.0.write_all(value)?)
    }
    fn u8(&mut self, value: u8) -> Result<()> {
        self.bytes(&[value])
    }
    fn u16(&mut self, value: u16) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn i64(&mut self, value: i64) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn text(&mut self, value: &str) -> Result<()> {
        let length = u16::try_from(value.len()).map_err(|_| Error::Corrupt("text too long"))?;
        self.u16(length)?;
        self.bytes(value.as_bytes())
    }
}

struct Input<R: Read>(R);

impl<R: Read> Input<R> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buffer = [0; N];
        self.0.read_exact(&mut buffer)?;
        Ok(buffer)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.bytes()?))
    }
    fn text(&mut self) -> Result<String> {
        let mut buffer = vec![0; self.u16()? as usize];
        self.0.read_exact(&mut buffer)?;
        String::from_utf8(buffer).map_err(|_| Error::Corrupt("text"))
    }
}

pub fn convert(container: &str, target: &Path) -> Result<()> {
    let mut tags = Vec::new();
    copy_rows(
        container,
        "SELECT id, key, type::text, count FROM tags",
        |fields| {
            tags.push(SnapshotTag {
                id: number(field(fields, 0)?)?,
                name: field(fields, 1)?.to_owned(),
                kind: kind_code(field(fields, 2)?),
                count: number(field(fields, 3)?)?,
            });
            Ok(())
        },
    )?;
    eprintln!("{} tags read", tags.len());

    let temporary = target.with_extension("partial");
    let mut output = Output(BufWriter::new(std::fs::File::create(&temporary)?));
    output.bytes(MAGIC)?;
    output.u32(tags.len() as u32)?;
    for tag in &tags {
        output.i64(tag.id)?;
        output.text(&tag.name)?;
        output.u8(tag.kind.unwrap_or(NO_KIND))?;
        output.i64(tag.count)?;
    }

    let sql = "SELECT g.gel_id, p.lewdness, p.score, (extract(epoch from g.posted_at) * 1000)::bigint, g.hash, g.original_ext, m.width, m.height, pt.tags \
               FROM gelbooru_posts g \
               JOIN posts p ON p.id = g.linked_to \
               JOIN post_tags pt ON pt.post = p.id \
               JOIN media m ON m.post = p.id AND m.quality = 2";
    let mut written: u64 = 0;
    copy_rows(container, sql, |fields| {
        let Some(rating) = rating_of(field(fields, 1)?) else {
            return Ok(());
        };
        let posted_at = field(fields, 3)?;
        let tags = tag_array(field(fields, 8)?)?;
        output.i64(number(field(fields, 0)?)?)?;
        output.u8(rating_code(rating))?;
        output.i64(number(field(fields, 2)?)?)?;
        output.i64(if posted_at == "\\N" {
            NO_DATE
        } else {
            number(posted_at)?
        })?;
        output.text(field(fields, 4)?)?;
        output.text(field(fields, 5)?)?;
        output.u32(number(field(fields, 6)?)?)?;
        output.u32(number(field(fields, 7)?)?)?;
        output.u16(u16::try_from(tags.len()).map_err(|_| Error::Corrupt("too many tags"))?)?;
        for tag in tags {
            output.u32(u32::try_from(tag).map_err(|_| Error::Corrupt("tag id"))?)?;
        }
        written += 1;
        if written.is_multiple_of(1_000_000) {
            eprintln!("{written} posts written");
        }
        Ok(())
    })?;
    output.0.flush()?;
    drop(output);
    std::fs::rename(&temporary, target)?;
    eprintln!("{written} posts written to {}", target.display());
    Ok(())
}

pub fn read(
    path: &Path,
    mut each_tag: impl FnMut(SnapshotTag),
    mut each_post: impl FnMut(SnapshotPost),
) -> Result<()> {
    let mut input = Input(BufReader::with_capacity(
        1 << 20,
        std::fs::File::open(path)?,
    ));
    if &input.bytes::<8>()? != MAGIC {
        return Err(Error::Corrupt("not a snapshot"));
    }
    for _ in 0..input.u32()? {
        let id = input.i64()?;
        let name = input.text()?;
        let kind = input.u8()?;
        let count = input.i64()?;
        each_tag(SnapshotTag {
            id,
            name,
            kind: (kind != NO_KIND).then_some(kind),
            count,
        });
    }
    loop {
        let id = match input.i64() {
            Ok(id) => id,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        };
        let rating = rating_from_code(input.u8()?)?;
        let score = input.i64()?;
        let posted_at = input.i64()?;
        let md5 = input.text()?;
        let ext = input.text()?;
        let width = input.u32()?;
        let height = input.u32()?;
        let count = input.u16()?;
        let tags = (0..count)
            .map(|_| input.u32().map(i64::from))
            .collect::<Result<_>>()?;
        each_post(SnapshotPost {
            id,
            rating,
            score,
            posted_at: (posted_at != NO_DATE).then_some(posted_at),
            md5,
            ext,
            width,
            height,
            tags,
        });
    }
    Ok(())
}
