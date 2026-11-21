// replays a real account's likes against the dump: the newest likes are hidden, For you ranks from the older ones,
// and the report shows where the hidden likes land, split by whether their artist was liked before
use crate::pool::Pool;
use foryooru_core::corpus::Corpus;
use foryooru_core::kinds::TagKind;
use foryooru_core::recommend::{self, Reaction, UserData};
use foryooru_core::search::Search;
use foryooru_core::signals::Signals;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;
use std::time::Instant;

// share of the likes, newest first, hidden from the ranking and looked for in it
const HELD_OUT: f64 = 0.2;
const DEPTHS: [usize; 4] = [60, 200, 1000, 2000];
const TOP: usize = 100;
// how far down For you to look for posts by the artists the account went on to like
const ARTIST_DEPTHS: [usize; 3] = [100, 500, 2000];
// fewer matched favourites than this hide too few posts to say anything
const MIN_LIKES: usize = 50;

#[derive(Deserialize)]
pub struct Reactions {
    // newest first
    likes: Vec<i64>,
    dislikes: Vec<i64>,
}

struct Recall {
    posts: usize,
    found: [usize; DEPTHS.len()],
}

struct Measured {
    earlier: usize,
    held: Recall,
    known_artist: Recall,
    new_artist: Recall,
    good_new_artists: usize,
    reached: [usize; ARTIST_DEPTHS.len()],
    top_artists: usize,
    by_known: usize,
}

fn artists(corpus: &Corpus, id: i64) -> Vec<i64> {
    corpus.row_of(id).map_or_else(Vec::new, |row| {
        corpus
            .tags_of_row(row)
            .into_iter()
            .filter(|tag| corpus.tags.get(tag).is_some_and(|info| info.kind == Some(TagKind::Artist)))
            .collect()
    })
}

fn recall(positions: &HashMap<i64, usize>, posts: &[i64]) -> Recall {
    Recall {
        posts: posts.len(),
        found: DEPTHS.map(|depth| {
            posts
                .iter()
                .filter(|id| positions.get(id).is_some_and(|position| *position < depth))
                .count()
        }),
    }
}

fn percent(found: usize, of: usize) -> f64 {
    found as f64 * 100.0 / of as f64
}

fn describe(recall: &Recall) -> String {
    if recall.posts == 0 {
        return "none".to_owned();
    }
    DEPTHS
        .iter()
        .zip(recall.found)
        .map(|(depth, found)| format!("top {depth}: {:.1}%", percent(found, recall.posts)))
        .collect::<Vec<_>>()
        .join(", ")
}

// likes must be newest first and already in the corpus
fn measure(corpus: &Corpus, likes: &[i64], dislikes: &[i64]) -> Measured {
    let held_count = (likes.len() as f64 * HELD_OUT).round() as usize;
    let (held, earlier) = likes.split_at(held_count);
    let known_artists: HashSet<i64> = earlier.iter().flat_map(|id| artists(corpus, *id)).collect();
    let (new_artist, known_artist): (Vec<i64>, Vec<i64>) = held
        .iter()
        .partition(|id| !artists(corpus, **id).iter().any(|artist| known_artists.contains(artist)));

    let user = UserData {
        likes: earlier.iter().map(|id| Reaction { id: *id }).collect(),
        dislikes: dislikes.iter().map(|id| Reaction { id: *id }).collect(),
        blacklist: Vec::new(),
        signals: Signals::default(),
        payoff: Default::default(),
        session: Vec::new(),
    };
    let ranked: Vec<i64> = recommend::for_you(corpus, &user, &Search::parse(&[]), true, false)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let positions: HashMap<i64, usize> = ranked.iter().enumerate().map(|(position, id)| (*id, position)).collect();

    let top = &ranked[..TOP.min(ranked.len())];
    let top_artists: HashSet<i64> = top.iter().flat_map(|id| artists(corpus, *id)).collect();
    let by_known = top
        .iter()
        .filter(|id| artists(corpus, **id).iter().any(|artist| known_artists.contains(artist)))
        .count();

    // the good new artists are the ones first liked among the hidden likes; any post of theirs counts as found
    let good_new_artists: HashSet<i64> = new_artist.iter().flat_map(|id| artists(corpus, *id)).collect();
    let reached = ARTIST_DEPTHS.map(|depth| {
        let shown: HashSet<i64> = ranked.iter().take(depth).flat_map(|id| artists(corpus, *id)).collect();
        good_new_artists.intersection(&shown).count()
    });

    Measured {
        earlier: earlier.len(),
        held: recall(&positions, held),
        known_artist: recall(&positions, &known_artist),
        new_artist: recall(&positions, &new_artist),
        good_new_artists: good_new_artists.len(),
        reached,
        top_artists: top_artists.len(),
        by_known,
    }
}

pub fn run(corpus: &Corpus, reactions: &Reactions) -> String {
    let likes: Vec<i64> = reactions.likes.iter().copied().filter(|id| corpus.has(*id)).collect();
    let measured = measure(corpus, &likes, &reactions.dislikes);

    let mut report = String::new();
    let _ = writeln!(
        report,
        "{} likes in the dump of {} ({} missing), {} dislikes; ranked from the older {}, looking for the newest {}",
        likes.len(),
        reactions.likes.len(),
        reactions.likes.len() - likes.len(),
        reactions.dislikes.len(),
        measured.earlier,
        measured.held.posts
    );
    let _ = writeln!(report, "- all hidden likes ({}): {}", measured.held.posts, describe(&measured.held));
    let _ = writeln!(
        report,
        "- by artists liked before ({}): {}",
        measured.known_artist.posts,
        describe(&measured.known_artist)
    );
    let _ = writeln!(
        report,
        "- by artists not liked before ({}): {}",
        measured.new_artist.posts,
        describe(&measured.new_artist)
    );
    let _ = writeln!(
        report,
        "- good new artists with a post in For you, of {}: {}",
        measured.good_new_artists,
        ARTIST_DEPTHS
            .iter()
            .zip(measured.reached)
            .map(|(depth, reached)| format!("top {depth}: {reached}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        report,
        "- top {TOP}: {} different artists, {} posts by artists liked before",
        measured.top_artists, measured.by_known
    );
    report
}

// the average over users of a share, each user counting the same however many likes they have
fn mean(values: impl Iterator<Item = Option<f64>>) -> String {
    let shares: Vec<f64> = values.flatten().collect();
    if shares.is_empty() {
        return "-".to_owned();
    }
    format!("{:.1}%", shares.iter().sum::<f64>() / shares.len() as f64)
}

fn share(found: usize, of: usize) -> Option<f64> {
    (of > 0).then(|| percent(found, of))
}

// each user's favourite md5s, newest first, from the SQLite file danbooru-favourites writes
fn read_favourites(path: &Path) -> rusqlite::Result<Vec<(String, Vec<u128>)>> {
    let db = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut query = db.prepare(
        "SELECT users.name, posts.md5 FROM favourites
         JOIN users ON users.id = favourites.user_id
         JOIN posts ON posts.id = favourites.post_id
         ORDER BY users.name, favourites.position",
    )?;
    let mut rows = query.query([])?;
    let mut dumps: Vec<(String, Vec<u128>)> = Vec::new();
    while let Some(row) = rows.next()? {
        let user: String = row.get(0)?;
        // Danbooru leaves md5 out on posts it hides
        let md5 = row.get::<_, Option<String>>(1)?.and_then(|md5| u128::from_str_radix(&md5, 16).ok());
        if dumps.last().is_none_or(|(last, _)| *last != user) {
            dumps.push((user, Vec::new()));
        }
        if let (Some(md5), Some((_, md5s))) = (md5, dumps.last_mut()) {
            md5s.push(md5);
        }
    }
    Ok(dumps)
}

// replays every user in a danbooru-favourites SQLite file, matching posts to the corpus by md5
pub fn run_users(pool: &Pool, favourites: &Path) -> std::io::Result<String> {
    let started = Instant::now();
    let dumps = read_favourites(favourites).map_err(std::io::Error::other)?;
    let wanted: HashSet<u128> = dumps.iter().flat_map(|(_, md5s)| md5s.iter().copied()).collect();
    let ids = pool.ids_by_md5(&wanted);
    eprintln!(
        "{} users, {} of {} favourites matched in {:.1}s",
        dumps.len(),
        ids.len(),
        wanted.len(),
        started.elapsed().as_secs_f64()
    );

    let mut report = String::new();
    let _ = writeln!(
        report,
        "Replayed {} Danbooru users, the newest {:.0}% of each one's matched favourites hidden. Averages count every user the same.\n",
        dumps.len(),
        HELD_OUT * 100.0
    );
    let _ = writeln!(
        report,
        "| user | matched | hidden: top 200 | top 1000 | new artists: top 1000 | good new artists in top 500 | ms |"
    );
    let _ = writeln!(report, "|---|---|---|---|---|---|---|");

    let mut measured_users: Vec<Measured> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (user, md5s) in &dumps {
        let likes: Vec<i64> = md5s.iter().filter_map(|md5| ids.get(md5).copied()).collect();
        if likes.len() < MIN_LIKES {
            skipped.push(format!("{user} ({} of {})", likes.len(), md5s.len()));
            continue;
        }
        let ranking_started = Instant::now();
        let measured = measure(&pool.corpus, &likes, &[]);
        let took = ranking_started.elapsed().as_millis();
        eprintln!("{user}: {} likes in {took}ms", likes.len());
        let cell = |share: Option<f64>| share.map_or("-".to_owned(), |share| format!("{share:.1}%"));
        let _ = writeln!(
            report,
            "| {user} | {} of {} | {} | {} | {} | {} of {} | {took} |",
            likes.len(),
            md5s.len(),
            cell(share(measured.held.found[1], measured.held.posts)),
            cell(share(measured.held.found[2], measured.held.posts)),
            cell(share(measured.new_artist.found[2], measured.new_artist.posts)),
            measured.reached[1],
            measured.good_new_artists
        );
        measured_users.push(measured);
    }

    let _ = writeln!(report, "\nAverage over {} users:", measured_users.len());
    let groups: [(&str, fn(&Measured) -> &Recall); 3] = [
        ("all hidden favourites", |measured| &measured.held),
        ("by artists favourited before", |measured| &measured.known_artist),
        ("by artists not favourited before", |measured| &measured.new_artist),
    ];
    for (name, recall_of) in groups {
        let depths = DEPTHS
            .iter()
            .enumerate()
            .map(|(index, depth)| {
                let shares = measured_users.iter().map(|measured| {
                    let recall = recall_of(measured);
                    share(recall.found[index], recall.posts)
                });
                format!("top {depth}: {}", mean(shares))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(report, "- {name}: {depths}");
    }
    let reached = ARTIST_DEPTHS
        .iter()
        .enumerate()
        .map(|(index, depth)| {
            let shares = measured_users
                .iter()
                .map(|measured| share(measured.reached[index], measured.good_new_artists));
            format!("top {depth}: {}", mean(shares))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(report, "- good new artists with a post in For you: {reached}");
    let _ = writeln!(
        report,
        "- top {TOP}: {:.0} different artists, {:.0} posts by artists favourited before",
        measured_users.iter().map(|measured| measured.top_artists).sum::<usize>() as f64 / measured_users.len().max(1) as f64,
        measured_users.iter().map(|measured| measured.by_known).sum::<usize>() as f64 / measured_users.len().max(1) as f64
    );
    if !skipped.is_empty() {
        let _ = writeln!(report, "\nSkipped with under {MIN_LIKES} matched favourites: {}", skipped.join(", "));
    }
    let _ = writeln!(report, "\nTook {:.0}s after loading the snapshot.", started.elapsed().as_secs_f64());
    Ok(report)
}
