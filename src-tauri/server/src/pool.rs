use crate::error::Result;
use crate::snapshot::{self, SnapshotPost};
use foryooru_core::corpus::{
    Corpus, NewPost, TagInfo, read_text, read_values, write_text, write_values,
};
use foryooru_core::kinds::{TagKind, is_still, media_of};
use foryooru_core::protocol::{ServedPost, ServedTag};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const CDN: &str = "https://img4.gelbooru.com";
const THUMBNAIL_SIDE: f64 = 250.0;
const SAMPLE_WIDTH: u32 = 850;

pub struct Pool {
    pub corpus: Corpus,
    md5s: Vec<u128>,
    exts: Vec<u8>,
    widths: Vec<u32>,
    heights: Vec<u32>,
    extensions: Vec<String>,
}

fn index_path(snapshot: &Path) -> PathBuf {
    snapshot.with_extension("index")
}

fn index_is_fresh(snapshot: &Path, index: &Path) -> bool {
    let modified = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    };
    matches!((modified(snapshot), modified(index)), (Some(built), Some(saved)) if saved >= built)
}

impl Pool {
    pub fn load(snapshot: &Path) -> Result<Pool> {
        let index = index_path(snapshot);
        if index_is_fresh(snapshot, &index) {
            eprintln!("reading the saved index");
            return Pool::read(&index);
        }
        let pool = Pool::build(snapshot)?;
        eprintln!("saving the index for faster restarts");
        pool.write(&index)?;
        Ok(pool)
    }

    fn read(index: &Path) -> Result<Pool> {
        let mut input = BufReader::with_capacity(1 << 22, File::open(index)?);
        let corpus = Corpus::load(&mut input)?;
        let md5s = read_values(&mut input)?;
        let exts = read_values(&mut input)?;
        let widths = read_values(&mut input)?;
        let heights = read_values(&mut input)?;
        let count = read_values::<u64>(&mut input)?
            .first()
            .copied()
            .unwrap_or(0);
        let extensions = (0..count)
            .map(|_| read_text(&mut input))
            .collect::<io::Result<_>>()?;
        Ok(Pool {
            corpus,
            md5s,
            exts,
            widths,
            heights,
            extensions,
        })
    }

    fn write(&self, index: &Path) -> Result<()> {
        let partial = index.with_extension("partial");
        let mut out = BufWriter::with_capacity(1 << 22, File::create(&partial)?);
        self.corpus.save(&mut out)?;
        write_values(&mut out, &self.md5s)?;
        write_values(&mut out, &self.exts)?;
        write_values(&mut out, &self.widths)?;
        write_values(&mut out, &self.heights)?;
        write_values(&mut out, &[self.extensions.len() as u64])?;
        for extension in &self.extensions {
            write_text(&mut out, extension)?;
        }
        out.flush()?;
        drop(out);
        std::fs::rename(&partial, index)?;
        Ok(())
    }

    fn build(path: &Path) -> Result<Pool> {
        let mut tags = Vec::new();
        let mut posts: Vec<SnapshotPost> = Vec::new();
        snapshot::read(
            path,
            |tag| tags.push(tag),
            |post| {
                posts.push(post);
                if posts.len().is_multiple_of(1_000_000) {
                    eprintln!("{} posts read", posts.len());
                }
            },
        )?;
        posts.sort_unstable_by_key(|post| post.id);
        let mut corpus = Corpus::new();
        corpus.set_tags(
            posts.len() as f64,
            tags.into_iter().map(|tag| {
                (
                    tag.id,
                    TagInfo {
                        name: tag.name,
                        kind: tag.kind.and_then(|code| TagKind::from_code(code as i64)),
                        count: Some(tag.count),
                    },
                )
            }),
        );
        let mut extensions: Vec<String> = Vec::new();
        let mut md5s = Vec::with_capacity(posts.len());
        let mut exts = Vec::with_capacity(posts.len());
        let mut widths = Vec::with_capacity(posts.len());
        let mut heights = Vec::with_capacity(posts.len());
        let mut new = Vec::with_capacity(posts.len());
        for post in posts {
            let ext = match extensions.iter().position(|known| *known == post.ext) {
                Some(index) => index as u8,
                None => {
                    extensions.push(post.ext.clone());
                    (extensions.len() - 1) as u8
                }
            };
            new.push(NewPost {
                id: post.id,
                rating: post.rating,
                score: post.score,
                md5: post.md5.clone(),
                posted_at: post.posted_at,
                media: media_of(&format!("file.{}", post.ext)),
                tags: post.tags,
            });
            md5s.push(u128::from_str_radix(&post.md5, 16).unwrap_or(0));
            exts.push(ext);
            widths.push(post.width);
            heights.push(post.height);
        }
        eprintln!("indexing {} posts", new.len());
        corpus.append(new);
        Ok(Pool {
            corpus,
            md5s,
            exts,
            widths,
            heights,
            extensions,
        })
    }

    pub fn served(&self, id: i64) -> Option<ServedPost> {
        let row = self.corpus.row_of(id)?;
        let md5 = format!("{:032x}", self.md5s[row]);
        let ext = &self.extensions[self.exts[row] as usize];
        let (width, height) = (self.widths[row], self.heights[row]);
        let directory = format!("{}/{}", md5.get(..2)?, md5.get(2..4)?);
        let file_url = format!("{CDN}/images/{directory}/{md5}.{ext}");
        let scale = (THUMBNAIL_SIDE / width.max(height).max(1) as f64).min(1.0);
        let sample_url = if width > SAMPLE_WIDTH && is_still(&file_url) {
            format!("{CDN}/samples/{directory}/sample_{md5}.jpg")
        } else {
            String::new()
        };
        Some(ServedPost {
            id,
            md5: md5.clone(),
            rating: self.corpus.rating_of_row(row),
            score: self.corpus.score_of_row(row),
            tags: self
                .corpus
                .tags_of_row(row)
                .into_iter()
                .filter_map(|tag| self.corpus.tag_name(tag).map(str::to_owned))
                .collect(),
            width: width as i64,
            height: height as i64,
            preview_url: format!("{CDN}/thumbnails/{directory}/thumbnail_{md5}.jpg"),
            preview_width: (width as f64 * scale).round() as i64,
            preview_height: (height as f64 * scale).round() as i64,
            sample_url,
            file_url,
            posted_at: self.corpus.posted_at_of_row(row),
        })
    }

    // matches posts from another site to the same files here
    pub fn ids_by_md5(&self, wanted: &HashSet<u128>) -> HashMap<u128, i64> {
        self.md5s
            .iter()
            .enumerate()
            .filter(|(_, md5)| wanted.contains(md5))
            .map(|(row, md5)| (*md5, self.corpus.ids[row]))
            .collect()
    }

    pub fn served_all(&self, ids: &[i64]) -> Vec<ServedPost> {
        ids.iter().filter_map(|id| self.served(*id)).collect()
    }

    pub fn tag(&self, name: &str) -> Option<ServedTag> {
        let id = self.corpus.tag_id(name)?;
        let info = self.corpus.tags.get(&id)?;
        Some(ServedTag {
            name: info.name.clone(),
            kind: info.kind.map(TagKind::code),
            count: info.count.unwrap_or(0),
        })
    }
}
