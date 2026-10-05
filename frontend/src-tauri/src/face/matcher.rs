//! Xotiradagi embedding indeksi. Vektorlar L2-normallangan, shuning uchun
//! cosine o'xshashlik = nuqta ko'paytma. Bir shaxsning bir nechta namunasi bo'lsa,
//! eng yaqini hisobga olinadi.

use serde::Serialize;

use crate::db::repo::StoredEmbedding;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub person_id: i64,
    pub score: f32,
}

pub struct Matcher {
    dim: usize,
    ids: Vec<i64>,
    /// Barcha vektorlar ketma-ket: keshga qulay va tez.
    data: Vec<f32>,
}

impl Matcher {
    pub fn new(dim: usize) -> Self {
        Self { dim, ids: Vec::new(), data: Vec::new() }
    }

    pub fn from_embeddings(dim: usize, rows: Vec<StoredEmbedding>) -> Self {
        let mut m = Self::new(dim);
        for r in rows {
            m.add(r.person_id, &r.vector);
        }
        m
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// O'lchami mos kelmagan vektor (boshqa model) e'tiborsiz qoldiriladi.
    pub fn add(&mut self, person_id: i64, v: &[f32]) -> bool {
        if v.len() != self.dim {
            return false;
        }
        self.ids.push(person_id);
        self.data.extend_from_slice(v);
        true
    }

    pub fn remove_person(&mut self, person_id: i64) {
        let dim = self.dim;
        let mut keep_ids = Vec::with_capacity(self.ids.len());
        let mut keep = Vec::with_capacity(self.data.len());
        for (i, &id) in self.ids.iter().enumerate() {
            if id != person_id {
                keep_ids.push(id);
                keep.extend_from_slice(&self.data[i * dim..(i + 1) * dim]);
            }
        }
        self.ids = keep_ids;
        self.data = keep;
    }

    /// Eng yaqin shaxs (chegarasiz). Ro'yxatga olishda dublikatni tekshirish uchun ham.
    pub fn nearest(&self, q: &[f32]) -> Option<Match> {
        if q.len() != self.dim {
            return None;
        }
        let mut best: Option<Match> = None;
        for (id, v) in self.ids.iter().zip(self.data.chunks_exact(self.dim)) {
            let score: f32 = v.iter().zip(q).map(|(a, b)| a * b).sum();
            if best.is_none_or(|b| score > b.score) {
                best = Some(Match { person_id: *id, score });
            }
        }
        best
    }
}

pub fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter_mut().for_each(|x| *x /= n);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: &[f32]) -> Vec<f32> {
        let mut v = v.to_vec();
        l2_normalize(&mut v);
        v
    }

    #[test]
    fn nearest_uses_best_sample_per_person() {
        let mut m = Matcher::new(2);
        m.add(1, &unit(&[1.0, 0.0]));
        m.add(2, &unit(&[0.0, 1.0]));
        m.add(2, &unit(&[1.0, 1.0]));
        let q = unit(&[0.9, 1.0]);
        let best = m.nearest(&q).unwrap();
        assert_eq!(best.person_id, 2);
        assert!(best.score > 0.99);
    }

    #[test]
    fn far_query_scores_low() {
        let mut m = Matcher::new(2);
        m.add(1, &unit(&[1.0, 0.0]));
        let q = unit(&[0.0, 1.0]);
        assert!(m.nearest(&q).unwrap().score < 0.36);
        assert_eq!(m.nearest(&q).unwrap().person_id, 1);
        assert!(Matcher::new(2).nearest(&q).is_none());
    }

    #[test]
    fn rejects_wrong_dim_and_removes_person() {
        let mut m = Matcher::new(2);
        assert!(!m.add(1, &[1.0, 0.0, 0.0]));
        m.add(1, &unit(&[1.0, 0.0]));
        m.add(2, &unit(&[0.0, 1.0]));
        m.add(1, &unit(&[1.0, 0.1]));
        m.remove_person(1);
        assert_eq!(m.len(), 1);
        assert_eq!(m.nearest(&unit(&[1.0, 0.0])).unwrap().person_id, 2);
        assert!(m.nearest(&[1.0]).is_none());
    }
}
