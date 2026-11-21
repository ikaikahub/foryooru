// the artists each artist's fans also like, built from scraped Danbooru favourites by danbooru-favourites/artist-table
// (its ARTISTS.md explains the scoring). names are Danbooru's, which Gelbooru mostly shares
use crate::tag_vectors::Reader;
use std::collections::HashMap;
use std::sync::OnceLock;

const BUNDLED: &[u8] = include_bytes!("../../assets/artist_neighbours.bin");
const MAGIC: &[u8] = b"FYAN";
// marks a neighbour slot left empty because the artist has fewer
const NO_NEIGHBOUR: i32 = -1;

pub struct ArtistNeighbours {
    names: Vec<String>,
    index: HashMap<String, usize>,
    per_artist: usize,
    // per_artist slots for each artist, best first
    neighbours: Vec<i32>,
}

impl ArtistNeighbours {
    // the artist's neighbours best first, or nothing for an artist the scrape didn't have enough fans for
    pub fn of(&self, name: &str) -> Vec<&str> {
        let Some(row) = self.index.get(name) else {
            return Vec::new();
        };
        self.neighbours[row * self.per_artist..(row + 1) * self.per_artist]
            .iter()
            .filter(|neighbour| **neighbour != NO_NEIGHBOUR)
            .filter_map(|neighbour| self.names.get(*neighbour as usize).map(String::as_str))
            .collect()
    }
}

pub fn bundled() -> &'static ArtistNeighbours {
    static NEIGHBOURS: OnceLock<ArtistNeighbours> = OnceLock::new();
    NEIGHBOURS.get_or_init(|| {
        parse(BUNDLED).unwrap_or(ArtistNeighbours {
            names: Vec::new(),
            index: HashMap::new(),
            per_artist: 0,
            neighbours: Vec::new(),
        })
    })
}

fn parse(bytes: &[u8]) -> Option<ArtistNeighbours> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.take(MAGIC.len())? != MAGIC {
        return None;
    }
    let count = reader.u32()?;
    let per_artist = reader.u32()?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let length = reader.u16()? as usize;
        names.push(std::str::from_utf8(reader.take(length)?).ok()?.to_owned());
    }
    let neighbours = reader
        .take(count * per_artist * 4)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| i32::from_le_bytes(*bytes))
        .collect();
    let index = names.iter().enumerate().map(|(row, name)| (name.clone(), row)).collect();
    Some(ArtistNeighbours {
        names,
        index,
        per_artist,
        neighbours,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_table_knows_well_known_artists() {
        let neighbours = bundled();
        assert!(neighbours.names.len() > 10_000);
        let similar = neighbours.of("nyantcha");
        assert!(!similar.is_empty() && !similar.contains(&"nyantcha"));
        assert!(neighbours.of("not_an_artist_anywhere").is_empty());
    }
}
