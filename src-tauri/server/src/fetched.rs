use foryooru_core::corpus::{Corpus, PAGE_SIZE};
use foryooru_core::feed::Leads;
use foryooru_core::fetch_plan::{self, GELBOORU_MAX_OFFSET, PostTag, TagFetch};
use foryooru_core::search::Order;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use std::collections::HashSet;

const MAX_PAGES: usize = GELBOORU_MAX_OFFSET as usize / PAGE_SIZE;

enum Wanted {
    Lead(String),
    Tag {
        name: String,
        order: Option<Order>,
        pages: usize,
    },
    Recent,
}

pub struct Fetched<'a> {
    site: &'a Corpus,
    pub local: Corpus,
    stored_pages: HashSet<(Option<i64>, usize)>,
    pending: Vec<Wanted>,
    pub requests: usize,
}

impl<'a> Fetched<'a> {
    pub fn new(site: &'a Corpus) -> Fetched<'a> {
        let mut local = Corpus::new();
        local.set_tags(
            site.site_total(),
            site.tags.iter().map(|(id, info)| (*id, info.clone())),
        );
        Fetched {
            site,
            local,
            stored_pages: HashSet::new(),
            pending: Vec::new(),
            requests: 0,
        }
    }

    pub fn store(&mut self, rows: impl IntoIterator<Item = usize>) {
        let new = rows
            .into_iter()
            .filter(|row| !self.local.has(self.site.ids[*row]))
            .map(|row| self.site.new_post_of_row(row))
            .collect();
        self.local.append(new);
    }

    fn post_tags(&self, id: i64) -> Vec<PostTag> {
        let Some(row) = self.local.row_of(id) else {
            return Vec::new();
        };
        self.local
            .tags_of_row(row)
            .into_iter()
            .filter_map(|tag| {
                let info = self.local.tags.get(&tag)?;
                Some(PostTag {
                    name: info.name.clone(),
                    kind: info.kind,
                    count: info.count,
                    stored: self.local.stored_posts(tag),
                })
            })
            .collect()
    }

    fn want_tag(&mut self, tag: &PostTag) {
        let TagFetch { order, pages } = fetch_plan::tag_fetch(tag);
        self.pending.push(Wanted::Tag {
            name: tag.name.clone(),
            order,
            pages,
        });
    }

    pub fn after_like(&mut self, id: i64) {
        let tags = self.post_tags(id);
        for tag in fetch_plan::targets(&tags) {
            self.want_tag(tag);
        }
    }

    pub fn after_long_looks(&mut self, ids: &[i64]) {
        let mut followed = HashSet::new();
        for id in ids {
            let tags = self.post_tags(*id);
            for tag in fetch_plan::followed_artists(&tags) {
                if followed.insert(tag.name.clone()) {
                    self.want_tag(tag);
                }
            }
        }
    }

    pub fn refill(&mut self, leads: Leads, likes: usize, rng: &mut StdRng) {
        let tags = fetch_plan::lead_tags(leads, likes, rng);
        if tags.is_empty() {
            self.pending.push(Wanted::Recent);
        }
        self.pending.extend(tags.into_iter().map(Wanted::Lead));
    }

    pub fn run_pending(&mut self, rng: &mut StdRng) {
        for wanted in std::mem::take(&mut self.pending) {
            let rows = self.fetch(wanted, rng);
            self.store(rows);
        }
    }

    fn unstored_pages(&self, key: Option<i64>, total: usize, limit: usize) -> Vec<usize> {
        (0..total.div_ceil(PAGE_SIZE).clamp(1, MAX_PAGES))
            .filter(|page| !self.stored_pages.contains(&(key, *page)))
            .take(limit)
            .collect()
    }

    fn pages_of(&mut self, key: Option<i64>, ordered: Vec<usize>, limit: usize) -> Vec<usize> {
        let pages = self.unstored_pages(key, ordered.len(), limit);
        let mut rows = Vec::new();
        for page in pages {
            self.requests += 1;
            self.stored_pages.insert((key, page));
            rows.extend(ordered.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE));
        }
        rows
    }

    fn fetch(&mut self, wanted: Wanted, rng: &mut StdRng) -> Vec<usize> {
        let site = self.site;
        let rows_of = |name: &str| {
            site.tag_id(name)
                .and_then(|tag| site.rows_with_tag(tag).map(|rows| (tag, rows)))
        };
        match wanted {
            Wanted::Lead(name) => {
                self.requests += 1;
                rows_of(&name).map_or_else(Vec::new, |(_, rows)| {
                    rows.choose_multiple(rng, PAGE_SIZE)
                        .map(|row| *row as usize)
                        .collect()
                })
            }
            Wanted::Tag { name, order, pages } => {
                let Some((tag, rows)) = rows_of(&name) else {
                    return Vec::new();
                };
                let mut ordered: Vec<usize> = rows.iter().map(|row| *row as usize).collect();
                match order {
                    Some(Order::Score) => {
                        ordered.sort_unstable_by_key(|row| -site.score_of_row(*row))
                    }
                    _ => ordered.sort_unstable_by_key(|row| -site.ids[*row]),
                }
                self.pages_of(Some(tag), ordered, pages)
            }
            Wanted::Recent => {
                let mut ordered: Vec<usize> = (0..site.size()).collect();
                let newest = (GELBOORU_MAX_OFFSET as usize).min(ordered.len()) - 1;
                ordered.select_nth_unstable_by_key(newest, |row| -site.ids[*row]);
                ordered.truncate(newest + 1);
                ordered.sort_unstable_by_key(|row| -site.ids[*row]);
                self.pages_of(None, ordered, 1)
            }
        }
    }
}
