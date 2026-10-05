//! Yangi shaxsni ro'yxatga olish sessiyasi. Kamera oqimi "enroll" rejimida har bir kadrni
//! shu yerga beradi: yuz joyi, o'lchami, pozasi va sifati tekshiriladi va shartlar
//! bajarilsa, namuna avtomatik olinadi (telefonda tugma bosish noqulay).

use std::io::Cursor;

use base64::Engine as _;
use image::{imageops, RgbImage};
use serde::Serialize;

use crate::db::repo::{self, NewEmbedding, Person, PersonInput};
use crate::error::{AppError, AppResult};
use crate::face::{align, matcher::Matcher, quality, Face, FaceEngine};

pub const TARGET_SAMPLES: usize = 5;
/// Yangi namuna birinchisiga kamida shuncha o'xshash bo'lishi kerak (o'sha odam ekanini tekshirish).
const SAME_PERSON_MIN: f32 = 0.40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Pose {
    Frontal,
    Left,
    Right,
    Up,
    Down,
}

pub const POSES: [Pose; TARGET_SAMPLES] = [Pose::Frontal, Pose::Left, Pose::Right, Pose::Up, Pose::Down];

impl Pose {
    /// Burchaklar taxminiy, shuning uchun oraliqlar keng olingan.
    fn matches(self, yaw: f32, pitch: f32) -> bool {
        match self {
            Pose::Frontal => yaw.abs() <= 12.0 && pitch.abs() <= 12.0,
            Pose::Left => (12.0..=45.0).contains(&yaw) && pitch.abs() <= 18.0,
            Pose::Right => (-45.0..=-12.0).contains(&yaw) && pitch.abs() <= 18.0,
            Pose::Up => (8.0..=40.0).contains(&pitch) && yaw.abs() <= 18.0,
            Pose::Down => (-40.0..=-8.0).contains(&pitch) && yaw.abs() <= 18.0,
        }
    }
}

/// Foydalanuvchiga ko'rsatiladigan maslahat (matni frontend'da).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Hint {
    NoFace,
    MultipleFaces,
    NotCentered,
    TooFar,
    TooClose,
    TurnPose,
    Blurry,
    LowScore,
    TooDark,
    TooBright,
    DifferentPerson,
    Hold,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    pub count: usize,
    pub target: Option<Pose>,
    pub hint: Option<Hint>,
    /// Shu kadrda namuna olindi.
    pub captured: bool,
    pub yaw_deg: Option<f32>,
    pub pitch_deg: Option<f32>,
    pub sharpness: Option<f32>,
}

#[derive(Clone)]
pub struct Sample {
    pub embedding: Vec<f32>,
    pub thumb_png: Vec<u8>,
    pub score: f32,
    pub pose: Option<Pose>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Duplicate {
    pub person_id: i64,
    pub person_name: String,
    pub score: f32,
}

#[derive(Default)]
pub struct Session {
    pub samples: Vec<Sample>,
    /// Hozirgi pozani o'tkazib yuborish (foydalanuvchi bajara olmasa): keyingi namuna har qanday pozada.
    skip_pose: bool,
    /// Ketma-ket mos kadrlar: bitta tasodifiy kadr emas, barqaror holat olinadi.
    stable: u32,
}

impl Session {
    pub fn target(&self) -> Option<Pose> {
        POSES.get(self.samples.len()).copied()
    }

    pub fn skip_pose(&mut self) {
        self.skip_pose = true;
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.samples.len() {
            self.samples.remove(index);
        }
    }

    fn feedback(&self, hint: Option<Hint>, captured: bool, q: Option<&quality::Quality>) -> Feedback {
        Feedback {
            count: self.samples.len(),
            target: self.target(),
            hint,
            captured,
            yaw_deg: q.map(|q| q.yaw_deg),
            pitch_deg: q.map(|q| q.pitch_deg),
            sharpness: q.map(|q| q.sharpness),
        }
    }

    /// Kamera kadri. `faces` — live detektor natijasi (160 px, taxminiy).
    pub fn process_frame(&mut self, engine: &FaceEngine, img: &RgbImage, faces: &[Face]) -> Feedback {
        if self.target().is_none() {
            return self.feedback(None, false, None);
        }
        let reject = |s: &mut Self, h: Hint, q: Option<&quality::Quality>| {
            s.stable = 0;
            s.feedback(Some(h), false, q)
        };
        let face = match faces {
            [] => return reject(self, Hint::NoFace, None),
            [f] => f,
            _ => return reject(self, Hint::MultipleFaces, None),
        };
        if let Some(h) = placement_hint(face, img.width() as f32, img.height() as f32) {
            return reject(self, h, None);
        }
        // Yuz atrofidagi kesmada qayta aniqlash: landmarklar ancha aniqroq chiqadi.
        let Some(face) = refine(engine, img, face) else { return reject(self, Hint::NoFace, None) };
        let (aligned, _) = align::align(img, &face.landmarks);
        let q = quality::assess(&face, &aligned, &capture_thresholds());
        if let Some(h) = quality_hint(&q) {
            return reject(self, h, Some(&q));
        }
        let target = self.target();
        if !self.skip_pose && target.is_some_and(|p| !p.matches(q.yaw_deg, q.pitch_deg)) {
            return reject(self, Hint::TurnPose, Some(&q));
        }
        // Ikki ketma-ket mos kadr: harakatdagi xira kadrni olmaslik uchun.
        self.stable += 1;
        if self.stable < 2 {
            return self.feedback(Some(Hint::Hold), false, Some(&q));
        }
        match self.capture(engine, &face, aligned, if self.skip_pose { None } else { target }) {
            Ok(true) => {
                self.stable = 0;
                self.skip_pose = false;
                self.feedback(None, true, Some(&q))
            }
            Ok(false) => reject(self, Hint::DifferentPerson, Some(&q)),
            Err(e) => {
                log::warn!("namuna olinmadi: {e}");
                reject(self, Hint::LowScore, Some(&q))
            }
        }
    }

    /// Galereyadagi rasm: eng katta yuz olinadi. Poza talab qilinmaydi.
    pub fn add_image(&mut self, engine: &FaceEngine, img: &RgbImage) -> AppResult<Result<(), Hint>> {
        if self.samples.len() >= TARGET_SAMPLES * 2 {
            return Err(AppError::Invalid("namunalar soni yetarli".into()));
        }
        let faces = engine.detector.detect(img)?;
        let Some(face) = faces.into_iter().max_by(|a, b| (a.bbox[2] * a.bbox[3]).total_cmp(&(b.bbox[2] * b.bbox[3]))) else {
            return Ok(Err(Hint::NoFace));
        };
        let (aligned, _) = align::align(img, &face.landmarks);
        let q = quality::assess(&face, &aligned, &quality::Thresholds::default());
        if let Some(h) = quality_hint(&q) {
            return Ok(Err(h));
        }
        Ok(if self.capture(engine, &face, aligned, None)? { Ok(()) } else { Err(Hint::DifferentPerson) })
    }

    /// `false` — namuna oldingilariga o'xshamaydi (boshqa odam).
    fn capture(&mut self, engine: &FaceEngine, face: &Face, aligned: RgbImage, pose: Option<Pose>) -> AppResult<bool> {
        let embedding = engine.embedder.embed(&aligned)?;
        if let Some(first) = self.samples.first() {
            if dot(&first.embedding, &embedding) < SAME_PERSON_MIN {
                return Ok(false);
            }
        }
        let mut thumb_png = Vec::new();
        aligned.write_to(&mut Cursor::new(&mut thumb_png), image::ImageFormat::Png)?;
        self.samples.push(Sample { embedding, thumb_png, score: face.score, pose });
        Ok(true)
    }

    /// Bazadagi eng o'xshash shaxs (chegaradan o'tsa). Barcha namunalar bo'yicha eng yuqori ball.
    pub fn duplicate(&self, matcher: &Matcher, threshold: f32) -> Option<(i64, f32)> {
        self.samples
            .iter()
            .filter_map(|s| matcher.nearest(&s.embedding))
            .filter(|m| m.score >= threshold)
            .max_by(|a, b| a.score.total_cmp(&b.score))
            .map(|m| (m.person_id, m.score))
    }

    /// Shaxs va namunalar bitta tranzaksiyada saqlanadi, keyin Matcher yangilanadi.
    pub fn commit(&self, conn: &mut rusqlite::Connection, matcher: &mut Matcher, input: &PersonInput) -> AppResult<Person> {
        if self.samples.is_empty() {
            return Err(AppError::Invalid("namuna olinmagan".into()));
        }
        let rows: Vec<NewEmbedding> = self
            .samples
            .iter()
            .map(|s| NewEmbedding {
                model: FaceEngine::MODEL_ID.into(),
                vector: s.embedding.clone(),
                quality: Some(s.score),
                thumb: Some(s.thumb_png.clone()),
                device: Some(device_tag()),
            })
            .collect();
        let person = repo::create_person(conn, input, &rows)?;
        for s in &self.samples {
            matcher.add(person.id, &s.embedding);
        }
        Ok(person)
    }
}

pub fn thumb_data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

fn device_tag() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Jonli kadrda sifat talablari biroz yumshoqroq: telefon kamerasi galereya rasmidan xiraroq.
fn capture_thresholds() -> quality::Thresholds {
    quality::Thresholds { min_face_width: 90.0, min_score: 0.8, min_sharpness: 25.0, max_yaw_deg: 90.0 }
}

fn quality_hint(q: &quality::Quality) -> Option<Hint> {
    use quality::QualityIssue::*;
    q.issues.iter().find_map(|i| match i {
        TooSmall => Some(Hint::TooFar),
        LowScore => Some(Hint::LowScore),
        Blurry => Some(Hint::Blurry),
        TooDark => Some(Hint::TooDark),
        TooBright => Some(Hint::TooBright),
        NotFrontal => None, // pozani alohida tekshiramiz
    })
}

/// Yuz ekrandagi oval ichida bo'lishi kerak: markazga yaqin va o'rtacha kattalikda.
fn placement_hint(f: &Face, w: f32, h: f32) -> Option<Hint> {
    let [x, y, bw, bh] = f.bbox;
    let (cx, cy) = (x + bw / 2.0, y + bh / 2.0);
    if (cx - w / 2.0).abs() > w * 0.18 || (cy - h * 0.45).abs() > h * 0.18 {
        return Some(Hint::NotCentered);
    }
    if bw < w * 0.22 {
        return Some(Hint::TooFar);
    }
    if bw > w * 0.8 || bh > h * 0.75 {
        return Some(Hint::TooClose);
    }
    None
}

/// Yuz atrofidagi kvadrat kesmani live detektorga berib, aniqroq landmarklar oladi.
fn refine(engine: &FaceEngine, img: &RgbImage, f: &Face) -> Option<Face> {
    let [x, y, bw, bh] = f.bbox;
    let side = (bw.max(bh) * 1.8).min(img.width().min(img.height()) as f32);
    let x0 = (x + bw / 2.0 - side / 2.0).clamp(0.0, img.width() as f32 - side) as u32;
    let y0 = (y + bh / 2.0 - side / 2.0).clamp(0.0, img.height() as f32 - side) as u32;
    let crop = imageops::crop_imm(img, x0, y0, side as u32, side as u32).to_image();
    let mut best = engine
        .live_detector
        .detect(&crop)
        .ok()?
        .into_iter()
        .max_by(|a, b| (a.bbox[2] * a.bbox[3]).total_cmp(&(b.bbox[2] * b.bbox[3])))?;
    best.bbox[0] += x0 as f32;
    best.bbox[1] += y0 as f32;
    for p in &mut best.landmarks {
        p[0] += x0 as f32;
        p[1] += y0 as f32;
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(cx: f32, cy: f32, w: f32) -> Face {
        Face { bbox: [cx - w / 2.0, cy - w / 2.0, w, w * 1.2], score: 0.9, landmarks: [[0.0; 2]; 5] }
    }

    #[test]
    fn placement_checks() {
        let (w, h) = (480.0, 640.0);
        assert_eq!(placement_hint(&face(240.0, 290.0, 200.0), w, h), None);
        assert_eq!(placement_hint(&face(60.0, 290.0, 200.0), w, h), Some(Hint::NotCentered));
        assert_eq!(placement_hint(&face(240.0, 290.0, 60.0), w, h), Some(Hint::TooFar));
        assert_eq!(placement_hint(&face(240.0, 290.0, 420.0), w, h), Some(Hint::TooClose));
    }

    #[test]
    fn pose_ranges() {
        assert!(Pose::Frontal.matches(3.0, -4.0));
        assert!(!Pose::Frontal.matches(20.0, 0.0));
        assert!(Pose::Left.matches(20.0, 5.0) && !Pose::Right.matches(20.0, 5.0));
        assert!(Pose::Right.matches(-20.0, 5.0));
        assert!(Pose::Up.matches(2.0, 15.0) && Pose::Down.matches(2.0, -15.0));
    }

    fn sample(v: &[f32]) -> Sample {
        let mut v = v.to_vec();
        crate::face::matcher::l2_normalize(&mut v);
        Sample { embedding: v, thumb_png: vec![1, 2, 3], score: 0.9, pose: Some(Pose::Frontal) }
    }

    #[test]
    fn duplicate_and_commit() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let mut matcher = Matcher::new(2);
        let input = PersonInput { full_name: "Ali".into(), ..Default::default() };
        let mut s = Session::default();
        assert!(s.commit(&mut conn, &mut matcher, &input).is_err(), "namunasiz saqlanmaydi");

        s.samples = vec![sample(&[1.0, 0.1]), sample(&[1.0, 0.0])];
        assert_eq!(s.duplicate(&matcher, 0.36), None);
        let p = s.commit(&mut conn, &mut matcher, &input).unwrap();
        assert_eq!(p.sample_count, 2);
        assert_eq!(matcher.len(), 2);
        assert_eq!(repo::load_embeddings(&conn, FaceEngine::MODEL_ID).unwrap().len(), 2);

        // Xuddi shu odam qayta qo'shilmoqchi bo'lsa — dublikat
        let again = Session { samples: vec![sample(&[0.99, 0.05])], ..Default::default() };
        let (id, score) = again.duplicate(&matcher, 0.36).unwrap();
        assert_eq!(id, p.id);
        assert!(score > 0.99);
        let other = Session { samples: vec![sample(&[0.0, 1.0])], ..Default::default() };
        assert_eq!(other.duplicate(&matcher, 0.36), None);
    }

    #[test]
    fn target_advances_and_remove() {
        let mut s = Session::default();
        assert_eq!(s.target(), Some(Pose::Frontal));
        s.samples = vec![sample(&[1.0, 0.0]); 2];
        assert_eq!(s.target(), Some(Pose::Right));
        s.remove(0);
        assert_eq!(s.target(), Some(Pose::Left));
        s.samples = vec![sample(&[1.0, 0.0]); TARGET_SAMPLES];
        assert_eq!(s.target(), None);
    }
}
