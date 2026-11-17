use crate::corpus::{ScoreFit, Taste};
use crate::engine::{Vector, dot, weighted_sum};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use std::collections::HashSet;

const MAX_GROUPS: usize = 8;
const LIKES_PER_GROUP_STEP: f64 = 5.0;
const MAX_ROUNDS: usize = 20;
const CLUSTER_SEED: u64 = 0;

#[derive(Clone)]
pub struct Interest {
    pub profile: Vector,
    pub share: f64,
    pub likes: usize,
    pub score_fit: Option<ScoreFit>,
}

impl Interest {
    pub fn taste(&self) -> Taste<'_> {
        Taste {
            profile: &self.profile,
            fit: self.score_fit,
        }
    }
}

pub fn group_count(likes: usize) -> usize {
    ((likes as f64 / LIKES_PER_GROUP_STEP).sqrt().round() as usize).clamp(1, MAX_GROUPS)
}

pub fn nearest(point: &Vector, centres: &[Vector]) -> (usize, f64) {
    centres
        .iter()
        .map(|centre| dot(point, centre))
        .enumerate()
        .fold((0, f64::NEG_INFINITY), |best, (index, closeness)| {
            if closeness > best.1 {
                (index, closeness)
            } else {
                best
            }
        })
}

fn first_centres(points: &[(Vector, f64)], count: usize, rng: &mut StdRng) -> Vec<Vector> {
    let indices: Vec<usize> = (0..points.len()).collect();
    let mut centres: Vec<Vector> = Vec::with_capacity(count);
    while centres.len() < count {
        let chosen = indices.choose_weighted(rng, |index| {
            let (vector, weight) = &points[*index];
            let distance = if centres.is_empty() {
                1.0
            } else {
                (1.0 - nearest(vector, &centres).1).max(0.0)
            };
            distance * weight
        });
        let Ok(chosen) = chosen else { break };
        centres.push(points[*chosen].0.clone());
    }
    centres
}

pub fn cluster_with_newcomers(
    points: &[(Vector, f64)],
    newcomers: usize,
    join_closeness: f64,
) -> Vec<Vec<usize>> {
    let newcomers = newcomers.min(points.len());
    let mut groups: Vec<Vec<usize>> = cluster(&points[newcomers..])
        .into_iter()
        .map(|members| {
            members
                .into_iter()
                .map(|member| member + newcomers)
                .collect()
        })
        .collect();
    for newer in (0..newcomers).rev() {
        let centres: Vec<Vector> = groups
            .iter()
            .map(|members| {
                weighted_sum(
                    members
                        .iter()
                        .map(|member| (&points[*member].0, points[*member].1)),
                )
            })
            .collect();
        let (best, closeness) = nearest(&points[newer].0, &centres);
        if closeness >= join_closeness {
            groups[best].push(newer);
        } else {
            groups.push(vec![newer]);
        }
    }
    groups
}

pub fn cluster(points: &[(Vector, f64)]) -> Vec<Vec<usize>> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut rng = StdRng::seed_from_u64(CLUSTER_SEED);
    let mut centres = first_centres(points, group_count(points.len()), &mut rng);
    let mut assigned: Vec<usize> = Vec::new();
    for _ in 0..MAX_ROUNDS {
        let next: Vec<usize> = points
            .iter()
            .map(|(vector, _)| nearest(vector, &centres).0)
            .collect();
        if next == assigned {
            break;
        }
        assigned = next;
        centres = (0..centres.len())
            .map(|group| {
                weighted_sum(
                    points
                        .iter()
                        .zip(&assigned)
                        .filter(|(_, member_of)| **member_of == group)
                        .map(|((vector, weight), _)| (vector, *weight)),
                )
            })
            .collect();
    }
    let mut groups = vec![Vec::new(); centres.len()];
    for (index, group) in assigned.into_iter().enumerate() {
        groups[group].push(index);
    }
    groups.retain(|members| !members.is_empty());
    groups
}

pub fn pick(interests: &[Interest], seed: u64) -> Option<usize> {
    let indices: Vec<usize> = (0..interests.len()).collect();
    indices
        .choose_weighted(&mut StdRng::seed_from_u64(seed), |index| {
            interests[*index].share
        })
        .ok()
        .copied()
}

pub fn weave(lists: Vec<Vec<i64>>, shares: &[f64]) -> Vec<i64> {
    let mut lists: Vec<std::vec::IntoIter<i64>> = lists.into_iter().map(Vec::into_iter).collect();
    let mut credit = vec![0.0; lists.len()];
    let mut open: Vec<bool> = vec![true; lists.len()];
    let mut placed = HashSet::new();
    let mut woven = Vec::new();
    loop {
        let total: f64 = (0..lists.len())
            .filter(|index| open[*index])
            .map(|index| shares[index])
            .sum();
        if total <= 0.0 {
            return woven;
        }
        for index in (0..lists.len()).filter(|index| open[*index]) {
            credit[index] += shares[index] / total;
        }
        let Some(turn) = (0..lists.len())
            .filter(|index| open[*index])
            .max_by(|a, b| credit[*a].total_cmp(&credit[*b]))
        else {
            return woven;
        };
        credit[turn] -= 1.0;
        match lists[turn].by_ref().find(|id| !placed.contains(id)) {
            Some(id) => {
                placed.insert(id);
                woven.push(id);
            }
            None => open[turn] = false,
        }
    }
}
