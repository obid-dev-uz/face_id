//! Real vaqtda tanish. Yuz har kadrda aniqlanadi va kadrlararo trekka bog'lanadi (IoU).
//! Embedding sekin (eski telefonda ~400 ms), shuning uchun u alohida ishchida va faqat
//! kerakli treklar uchun hisoblanadi: avval yangilari, keyin vaqti-vaqti bilan qayta
//! tekshirish. Trek bo'yicha bir nechta natijadan ovoz berib, shaxs aniqlanadi.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use image::RgbImage;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::camera::Slot;
use crate::db::repo::{self, NewLog};
use crate::face::{align, matcher::Match, quality, Face};
use crate::state::AppState;

/// Bir kadrdan keyingisiga o'tgan yuz shu qadar ustma-ust tushsa, o'sha trek.
const IOU_MIN: f32 = 0.3;
/// Shuncha kadr ko'rinmasa, trek o'chiriladi (yuz bir lahzaga yo'qolishi mumkin).
const MAX_MISSED: u32 = 6;
const VOTES: usize = 5;
/// Bitta natija chegaradan shuncha yuqori bo'lsa, ovoz kutmasdan tanildi.
const STRONG_MARGIN: f32 = 0.12;
/// Shuncha natijadan keyin ham hech kim tanilmasa — "Noma'lum".
const UNKNOWN_AFTER: usize = 3;
/// Juda kichik yuzdan embedding ishonchsiz: tanishga urinilmaydi.
const MIN_FACE_PX: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Pending,
    Known,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackView {
    pub id: u32,
    pub bbox: [f32; 4],
    pub status: Status,
    pub person_id: Option<i64>,
    pub name: Option<String>,
    pub score: Option<f32>,
}

/// Bitta embedding natijasi: `person_id` faqat chegaradan o'tgan bo'lsa.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Vote {
    person_id: Option<i64>,
    score: f32,
}

/// Trek holati o'zgardi: jurnalga yozish uchun.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decision {
    pub track_id: u32,
    pub person_id: Option<i64>,
    pub score: f32,
}

struct Track {
    id: u32,
    bbox: [f32; 4],
    /// Shu kadrdagi yuz (ko'rinmasa — `None`).
    face: Option<Face>,
    missed: u32,
    votes: VecDeque<Vote>,
    status: Status,
    person: Option<(i64, f32)>,
    last_job: Option<Instant>,
    jobs: u32,
}

#[derive(Default)]
pub struct Tracker {
    tracks: Vec<Track>,
    next_id: u32,
    names: HashMap<i64, String>,
    /// Embedding o'rtacha vaqti: qayta tekshirish oralig'i qurilma kuchiga moslashadi.
    embed_ms: f32,
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    let union = a[2] * a[3] + b[2] * b[3] - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Ovozlardan holat: kuchli bitta natija yoki ikki marta bir xil shaxs — tanildi;
/// yetarli urinishdan keyin ham hech kim — noma'lum.
fn decide(votes: &VecDeque<Vote>, threshold: f32) -> (Status, Option<(i64, f32)>) {
    if let Some(&Vote { person_id: Some(p), score }) = votes.back() {
        if score >= threshold + STRONG_MARGIN {
            return (Status::Known, Some((p, score)));
        }
    }
    let mut counts: HashMap<i64, (usize, f32)> = HashMap::new();
    for v in votes {
        if let Some(p) = v.person_id {
            let e = counts.entry(p).or_insert((0, 0.0));
            e.0 += 1;
            e.1 = e.1.max(v.score);
        }
    }
    let best = counts.into_iter().max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(a.1 .1.total_cmp(&b.1 .1)));
    match best {
        Some((p, (n, score))) if n >= 2 => (Status::Known, Some((p, score))),
        _ if votes.len() >= UNKNOWN_AFTER => (Status::Unknown, None),
        _ => (Status::Pending, None),
    }
}

impl Tracker {
    pub fn reset(&mut self) {
        self.tracks.clear();
    }

    /// Yangi kadrdagi yuzlarni treklarga bog'laydi (ochko'z IoU).
    pub fn update(&mut self, faces: &[Face]) {
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (ti, t) in self.tracks.iter().enumerate() {
            for (fi, f) in faces.iter().enumerate() {
                let v = iou(&t.bbox, &f.bbox);
                if v >= IOU_MIN {
                    pairs.push((v, ti, fi));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut used_t = vec![false; self.tracks.len()];
        let mut used_f = vec![false; faces.len()];
        for t in &mut self.tracks {
            t.face = None;
        }
        for (_, ti, fi) in pairs {
            if used_t[ti] || used_f[fi] {
                continue;
            }
            used_t[ti] = true;
            used_f[fi] = true;
            let t = &mut self.tracks[ti];
            t.bbox = faces[fi].bbox;
            t.face = Some(faces[fi].clone());
            t.missed = 0;
        }
        for t in &mut self.tracks {
            if t.face.is_none() {
                t.missed += 1;
            }
        }
        self.tracks.retain(|t| t.missed <= MAX_MISSED);
        for (fi, f) in faces.iter().enumerate() {
            if !used_f[fi] {
                self.next_id = self.next_id.wrapping_add(1);
                self.tracks.push(Track {
                    id: self.next_id,
                    bbox: f.bbox,
                    face: Some(f.clone()),
                    missed: 0,
                    votes: VecDeque::new(),
                    status: Status::Pending,
                    person: None,
                    last_job: None,
                    jobs: 0,
                });
            }
        }
    }

    /// Tanilgan trek ham vaqti-vaqti bilan qayta tekshiriladi (yuz almashib qolishi mumkin).
    fn refresh_every(&self) -> Duration {
        Duration::from_millis((self.embed_ms * 5.0).clamp(1500.0, 5000.0) as u64)
    }

    /// Embedding uchun navbatdagi trek: avval hali aniqlanmaganlar (eng kam urinilgani),
    /// keyin qayta tekshirish vaqti kelganlar.
    pub fn next_job(&mut self, now: Instant) -> Option<(u32, Face)> {
        let refresh = self.refresh_every();
        let ready = |t: &&mut Track| t.face.as_ref().is_some_and(|f| f.bbox[2] >= MIN_FACE_PX);
        let t = {
            let mut pending: Vec<&mut Track> =
                self.tracks.iter_mut().filter(ready).filter(|t| t.status == Status::Pending).collect();
            pending.sort_by_key(|t| t.jobs);
            match pending.into_iter().next() {
                Some(t) => t,
                None => self
                    .tracks
                    .iter_mut()
                    .filter(ready)
                    .filter(|t| t.last_job.is_none_or(|at| now.duration_since(at) >= refresh))
                    .min_by_key(|t| t.last_job)?,
            }
        };
        t.last_job = Some(now);
        t.jobs += 1;
        Some((t.id, t.face.clone()?))
    }

    /// Embedding natijasi. Holat o'zgarsa (yangi shaxs tanildi yoki "noma'lum" bo'ldi),
    /// jurnal uchun qaror qaytariladi.
    pub fn vote(&mut self, track_id: u32, nearest: Option<Match>, threshold: f32) -> Option<Decision> {
        let t = self.tracks.iter_mut().find(|t| t.id == track_id)?;
        let score = nearest.map_or(0.0, |m| m.score);
        let person_id = nearest.filter(|m| m.score >= threshold).map(|m| m.person_id);
        t.votes.push_back(Vote { person_id, score });
        if t.votes.len() > VOTES {
            t.votes.pop_front();
        }
        let (status, person) = decide(&t.votes, threshold);
        let changed = status != t.status || person.map(|p| p.0) != t.person.map(|p| p.0);
        t.status = status;
        t.person = person;
        match status {
            Status::Pending => None,
            _ if !changed => None,
            _ => Some(Decision { track_id, person_id: person.map(|p| p.0), score: person.map_or(score, |p| p.1) }),
        }
    }

    pub fn set_embed_ms(&mut self, ms: f32) {
        self.embed_ms = if self.embed_ms == 0.0 { ms } else { self.embed_ms * 0.8 + ms * 0.2 };
    }

    /// Shu kadrda ko'rinayotgan treklar.
    pub fn views(&self) -> Vec<TrackView> {
        self.tracks
            .iter()
            .filter(|t| t.face.is_some())
            .map(|t| TrackView {
                id: t.id,
                bbox: t.bbox,
                status: t.status,
                person_id: t.person.map(|p| p.0),
                name: t.person.and_then(|p| self.names.get(&p.0).cloned()),
                score: t.person.map(|p| p.1),
            })
            .collect()
    }
}

/// Jurnalni to'ldirib yubormaslik: bir shaxs `cooldown` ichida bir marta,
/// noma'lumlar esa umumiy qisqaroq oraliq bilan.
pub struct LogGate {
    cooldown: Duration,
    last: HashMap<Option<i64>, Instant>,
}

impl LogGate {
    pub fn new(cooldown: Duration) -> Self {
        Self { cooldown, last: HashMap::new() }
    }

    pub fn allow(&mut self, person_id: Option<i64>, now: Instant) -> bool {
        let gap = if person_id.is_some() { self.cooldown } else { self.cooldown / 4 };
        if self.last.get(&person_id).is_some_and(|&at| now.duration_since(at) < gap) {
            return false;
        }
        self.last.insert(person_id, now);
        true
    }
}

pub struct Job {
    pub track_id: u32,
    pub frame: RgbImage,
    pub face: Face,
    pub source: &'static str,
}

/// Oqimga biriktirilgan tanish ishchisi. Bir vaqtda bitta embedding: telefon protsessori
/// kamera oqimi bilan bo'lishiladi.
#[derive(Default)]
pub struct Recognizer {
    pub tracker: Arc<Mutex<Tracker>>,
    jobs: Slot<Job>,
    busy: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Recognizer {
    pub fn start(&self, app: AppHandle, running: Arc<AtomicBool>) {
        self.tracker.lock().reset();
        self.jobs.clear();
        self.busy.store(false, Ordering::Release);
        let (tracker, jobs, busy) = (self.tracker.clone(), self.jobs.clone(), self.busy.clone());
        let handle = std::thread::Builder::new()
            .name("khoface-recognize".into())
            .spawn(move || run(app, tracker, jobs, busy, running))
            .expect("recognize thread");
        *self.worker.lock() = Some(handle);
    }

    /// `running` oqim bilan umumiy: u allaqachon `false` qilingan bo'lishi kerak.
    pub fn join(&self) {
        if let Some(h) = self.worker.lock().take() {
            let _ = h.join();
        }
    }

    /// Kadrni trekerga beradi va ishchi bo'sh bo'lsa, navbatdagi embedding vazifasini yuboradi.
    pub fn on_frame(&self, frame: &RgbImage, faces: &[Face], source: &'static str) -> Vec<TrackView> {
        let mut t = self.tracker.lock();
        t.update(faces);
        if !self.busy.load(Ordering::Acquire) {
            if let Some((track_id, face)) = t.next_job(Instant::now()) {
                self.busy.store(true, Ordering::Release);
                self.jobs.put(Job { track_id, frame: frame.clone(), face, source });
            }
        }
        t.views()
    }
}

fn setting<T: std::str::FromStr>(state: &AppState, key: &str, default: T) -> T {
    repo::get_setting(&state.db.lock(), key).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn run(app: AppHandle, tracker: Arc<Mutex<Tracker>>, jobs: Slot<Job>, busy: Arc<AtomicBool>, running: Arc<AtomicBool>) {
    let state = app.state::<AppState>();
    let cooldown = setting(&state, "log.cooldown_s", 60u64);
    let mut gate = LogGate::new(Duration::from_secs(cooldown));
    while running.load(Ordering::Acquire) {
        let Some(job) = jobs.wait_take(Duration::from_millis(200)) else { continue };
        if let Err(e) = process(&app, &state, &tracker, &mut gate, job) {
            log::warn!("tanish: {e:#}");
        }
        busy.store(false, Ordering::Release);
    }
}

fn process(app: &AppHandle, state: &AppState, tracker: &Mutex<Tracker>, gate: &mut LogGate, job: Job) -> anyhow::Result<()> {
    let engine = state.engine()?;
    let threshold = setting(state, "match.threshold", crate::commands::engine::DEFAULT_THRESHOLD);
    // Kesmada qayta aniqlash: tekislash uchun aniqroq landmarklar. Topilmasa — keyingi safar.
    let Some(face) = engine.refine(&job.frame, &job.face) else { return Ok(()) };
    let (aligned, _) = align::align(&job.frame, &face.landmarks);
    let q = quality::assess(&face, &aligned, &quality::Thresholds { min_face_width: MIN_FACE_PX, min_score: 0.7, min_sharpness: 15.0, max_yaw_deg: 45.0 });
    if !q.issues.is_empty() {
        return Ok(()); // xira yoki burilgan yuz — ovoz bermaydi
    }
    let t0 = Instant::now();
    let embedding = engine.embedder.embed(&aligned)?;
    let embed_ms = t0.elapsed().as_secs_f32() * 1000.0;
    let nearest = state.matcher.read().nearest(&embedding);
    // Ism trekerga ovozdan oldin tushadi — ramkada darhol ko'rinadi.
    let name = match nearest.filter(|m| m.score >= threshold) {
        Some(m) if !tracker.lock().names.contains_key(&m.person_id) => {
            repo::get_person(&state.db.lock(), m.person_id).ok().map(|p| (m.person_id, p.full_name))
        }
        _ => None,
    };
    let decision = {
        let mut t = tracker.lock();
        t.set_embed_ms(embed_ms);
        if let Some((id, n)) = name {
            t.names.insert(id, n);
        }
        t.vote(job.track_id, nearest, threshold)
    };
    let Some(d) = decision else { return Ok(()) };
    if !gate.allow(d.person_id, Instant::now()) {
        return Ok(());
    }
    let mut snapshot = Vec::new();
    JpegEncoder::new_with_quality(&mut snapshot, 80).encode_image(&aligned)?;
    repo::insert_log(
        &state.db.lock(),
        &NewLog { person_id: d.person_id, score: d.score, matched: d.person_id.is_some(), source: job.source.into(), snapshot: Some(snapshot) },
    )?;
    let _ = app.emit("log-added", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x: f32, y: f32, w: f32) -> Face {
        Face { bbox: [x, y, w, w], score: 0.9, landmarks: [[0.0; 2]; 5] }
    }

    fn m(person_id: i64, score: f32) -> Option<Match> {
        Some(Match { person_id, score })
    }

    #[test]
    fn tracks_follow_moving_face_and_expire() {
        let mut t = Tracker::default();
        t.update(&[face(100.0, 100.0, 100.0)]);
        let id = t.views()[0].id;
        t.update(&[face(110.0, 105.0, 100.0)]);
        assert_eq!(t.views().len(), 1);
        assert_eq!(t.views()[0].id, id, "siljigan yuz — o'sha trek");
        t.update(&[face(400.0, 100.0, 100.0)]);
        assert_ne!(t.views()[0].id, id, "uzoqdagi yuz — yangi trek");
        for _ in 0..=MAX_MISSED {
            t.update(&[]);
        }
        assert!(t.tracks.is_empty());
    }

    #[test]
    fn pending_tracks_get_jobs_first_then_refresh() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.update(&[face(0.0, 0.0, 100.0), face(300.0, 0.0, 100.0), face(600.0, 0.0, 20.0)]);
        let (a, _) = t.next_job(now).unwrap();
        let (b, _) = t.next_job(now).unwrap();
        assert_ne!(a, b);
        assert!(t.vote(a, m(7, 0.9), 0.36).is_some());
        assert!(t.vote(b, m(8, 0.9), 0.36).is_some());
        // Kichik yuz tanishga berilmaydi, tanilganlar esa hali yangilanmaydi
        assert!(t.next_job(now).is_none());
        assert!(t.next_job(now + Duration::from_secs(6)).is_some());
    }

    #[test]
    fn voting_rules() {
        let mut t = Tracker::default();
        t.update(&[face(0.0, 0.0, 100.0)]);
        let id = t.views()[0].id;
        // Chegaradan biroz yuqori bitta natija — hali yetmaydi
        assert_eq!(t.vote(id, m(1, 0.40), 0.36), None);
        assert_eq!(t.views()[0].status, Status::Pending);
        // Ikkinchi marta o'sha shaxs — tanildi
        let d = t.vote(id, m(1, 0.42), 0.36).unwrap();
        assert_eq!((d.person_id, d.score), (Some(1), 0.42));
        assert_eq!(t.views()[0].status, Status::Known);
        // Holat o'zgarmasa, qayta yozilmaydi
        assert_eq!(t.vote(id, m(1, 0.5), 0.36), None);

        let mut t = Tracker::default();
        t.update(&[face(0.0, 0.0, 100.0)]);
        let id = t.views()[0].id;
        assert_eq!(t.vote(id, m(1, 0.2), 0.36), None);
        assert_eq!(t.vote(id, None, 0.36), None);
        let d = t.vote(id, m(2, 0.3), 0.36).unwrap();
        assert_eq!(d.person_id, None, "uch urinishda hech kim — noma'lum");
        // Kuchli natija darhol tanitadi
        assert_eq!(t.vote(id, m(3, 0.6), 0.36).unwrap().person_id, Some(3));
    }

    #[test]
    fn log_gate_throttles_per_person() {
        let mut g = LogGate::new(Duration::from_secs(60));
        let now = Instant::now();
        assert!(g.allow(Some(1), now));
        assert!(!g.allow(Some(1), now + Duration::from_secs(30)));
        assert!(g.allow(Some(2), now + Duration::from_secs(30)));
        assert!(g.allow(Some(1), now + Duration::from_secs(61)));
        assert!(g.allow(None, now));
        assert!(!g.allow(None, now + Duration::from_secs(10)));
        assert!(g.allow(None, now + Duration::from_secs(16)));
    }
}
