//! Yangi shaxsni ro'yxatga olish sessiyasi. Kamera oqimi "enroll" rejimida har bir kadrni
//! shu yerga beradi: yuz joyi, o'lchami, pozasi va sifati tekshiriladi va shartlar
//! bajarilsa, namuna avtomatik olinadi (telefonda tugma bosish noqulay). Avval to'g'ri
//! qaragan namuna, keyin foydalanuvchi boshini aylantiradi va har bir yo'nalishda bittadan.

use std::io::Cursor;

use base64::Engine as _;
use image::RgbImage;
use serde::Serialize;

use crate::db::repo::{self, NewEmbedding, Person, PersonInput};
use crate::error::{AppError, AppResult};
use crate::face::{align, matcher::Matcher, quality, Face, FaceEngine};

/// Yangi namuna birinchisiga kamida shuncha o'xshash bo'lishi kerak (o'sha odam ekanini tekshirish).
const SAME_PERSON_MIN: f32 = 0.40;
/// Yo'nalish shuncha ketma-ket kadrda barqaror bo'lsa, namuna olinadi (harakatdagi xira kadr emas).
const STABLE_FRAMES: u32 = 2;
/// Aylana chegaralari (to'g'ri qarashga nisbatan, gradus). Pitch landmarklardan kichikroq
/// chiqadi, shuning uchun uning ostonasi pastroq: (yaw/MIN_YAW, pitch/MIN_PITCH) ellips.
const MIN_YAW: f32 = 12.0;
const MIN_PITCH: f32 = 7.0;
const MAX_ANGLE: f32 = 45.0;
/// Diagonal yo'nalishlarda bosh ikki o'q bo'yicha bir vaqtda buriladi — bu qiyinroq, shuning
/// uchun ularning sektori kengroq (to'g'ri yo'nalishlar ±15°, diagonallar 60°) va
/// ellips bo'yicha kerakli masofa kichikroq.
const CARDINAL_HALF_DEG: f32 = 15.0;
const DIAGONAL_MIN_R: f32 = 0.75;

/// To'g'ri qarash va bosh aylanasi bo'ylab 8 yo'nalish (Face ID kabi: foydalanuvchi boshini
/// aylantiradi, har bir sektordan o'tganda bittadan namuna olinadi).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Pose {
    Frontal,
    Left,
    UpLeft,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
}

/// Soat miliga teskari, 45° qadam bilan; 0° — yaw musbat (foydalanuvchining chapi).
pub const DIRECTIONS: [Pose; 8] =
    [Pose::Left, Pose::UpLeft, Pose::Up, Pose::UpRight, Pose::Right, Pose::DownRight, Pose::Down, Pose::DownLeft];

pub const TARGET_SAMPLES: usize = 1 + DIRECTIONS.len();

impl Pose {
    fn is_frontal(yaw: f32, pitch: f32) -> bool {
        yaw.abs() <= MIN_YAW && pitch.abs() <= 14.0
    }

    /// Nisbiy burchaklar qaysi sektorga tushadi. Markazga juda yaqin yoki haddan tashqari
    /// burilgan bo'lsa — `None`.
    fn direction(yaw: f32, pitch: f32) -> Option<Pose> {
        if yaw.abs() > MAX_ANGLE || pitch.abs() > MAX_ANGLE {
            return None;
        }
        let (x, y) = (yaw / MIN_YAW, pitch / MIN_PITCH);
        let a = y.atan2(x).to_degrees().rem_euclid(360.0);
        let nearest = (a / 90.0).round() * 90.0;
        let cardinal = (a - nearest).abs() <= CARDINAL_HALF_DEG;
        if x.hypot(y) < if cardinal { 1.0 } else { DIAGONAL_MIN_R } {
            return None;
        }
        let sector = if cardinal { (nearest / 45.0) as usize } else { (a / 90.0).floor() as usize * 2 + 1 };
        Some(DIRECTIONS[sector % 8])
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
    /// Aylanadagi olingan yo'nalishlar (ekrandagi halqa uchun).
    pub covered: Vec<Pose>,
    /// Shu kadrda namuna olindi.
    pub captured: bool,
    /// To'g'ri qaragan namunaga nisbatan (u hali olinmagan bo'lsa — mutlaq).
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
    /// Olingan paytdagi mutlaq burchaklar.
    pub yaw_deg: f32,
    pub pitch_deg: f32,
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
    /// Ketma-ket mos kadrlar soni va ular qaysi yo'nalishga tegishli.
    stable: u32,
    pending: Option<Pose>,
}

impl Session {
    fn has(&self, p: Pose) -> bool {
        self.samples.iter().any(|s| s.pose == Some(p))
    }

    /// Avval to'g'ri qarash, keyin aylanadagi hali olinmagan birinchi yo'nalish;
    /// namunalar yetarli bo'lsa — `None`.
    pub fn target(&self) -> Option<Pose> {
        if self.samples.len() >= TARGET_SAMPLES {
            return None;
        }
        if !self.has(Pose::Frontal) {
            return Some(Pose::Frontal);
        }
        DIRECTIONS.iter().copied().find(|&p| !self.has(p))
    }

    pub fn covered(&self) -> Vec<Pose> {
        DIRECTIONS.iter().copied().filter(|&p| self.has(p)).collect()
    }

    /// To'g'ri qaragan namunadagi burchaklar — boshqa pozalar shunga nisbatan.
    fn base(&self) -> (f32, f32) {
        self.samples
            .iter()
            .find(|s| s.pose == Some(Pose::Frontal))
            .map_or((0.0, 0.0), |s| (s.yaw_deg, s.pitch_deg))
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.samples.len() {
            self.samples.remove(index);
        }
    }

    fn feedback(&self, hint: Option<Hint>, captured: bool, q: Option<&quality::Quality>) -> Feedback {
        let (by, bp) = self.base();
        Feedback {
            count: self.samples.len(),
            target: self.target(),
            hint,
            covered: self.covered(),
            captured,
            yaw_deg: q.map(|q| q.yaw_deg - by),
            pitch_deg: q.map(|q| q.pitch_deg - bp),
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
        // Bosh aylanayotganda yuz ovaldan biroz chiqadi — vertikal chegara kengroq.
        let around = self.has(Pose::Frontal);
        if let Some(h) = placement_hint(face, img.width() as f32, img.height() as f32, around) {
            return reject(self, h, None);
        }
        // Yuz atrofidagi kesmada qayta aniqlash: landmarklar ancha aniqroq chiqadi.
        let Some(face) = engine.refine(img, face) else { return reject(self, Hint::NoFace, None) };
        let (aligned, _) = align::align(img, &face.landmarks);
        let q = quality::assess(&face, &aligned, &capture_thresholds());
        if let Some(h) = quality_hint(&q) {
            return reject(self, h, Some(&q));
        }
        let (by, bp) = self.base();
        let (yaw, pitch) = (q.yaw_deg - by, q.pitch_deg - bp);
        let pose = if around {
            Pose::direction(yaw, pitch).filter(|&p| !self.has(p))
        } else {
            Pose::is_frontal(yaw, pitch).then_some(Pose::Frontal)
        };
        let Some(pose) = pose else { return reject(self, Hint::TurnPose, Some(&q)) };
        if self.pending != Some(pose) {
            self.pending = Some(pose);
            self.stable = 0;
        }
        self.stable += 1;
        if self.stable < STABLE_FRAMES {
            return self.feedback(Some(Hint::Hold), false, Some(&q));
        }
        match self.capture(engine, &face, aligned, Some(pose), &q) {
            Ok(true) => {
                self.stable = 0;
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
        Ok(if self.capture(engine, &face, aligned, None, &q)? { Ok(()) } else { Err(Hint::DifferentPerson) })
    }

    /// `false` — namuna oldingilariga o'xshamaydi (boshqa odam).
    fn capture(
        &mut self,
        engine: &FaceEngine,
        face: &Face,
        aligned: RgbImage,
        pose: Option<Pose>,
        q: &quality::Quality,
    ) -> AppResult<bool> {
        let embedding = engine.embedder.embed(&aligned)?;
        if let Some(first) = self.samples.first() {
            if dot(&first.embedding, &embedding) < SAME_PERSON_MIN {
                return Ok(false);
            }
        }
        let mut thumb_png = Vec::new();
        aligned.write_to(&mut Cursor::new(&mut thumb_png), image::ImageFormat::Png)?;
        self.samples.push(Sample {
            embedding,
            thumb_png,
            score: face.score,
            pose,
            yaw_deg: q.yaw_deg,
            pitch_deg: q.pitch_deg,
        });
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
/// `tilt` — bosh aylanayotgan bosqich: vertikal chegara kengroq.
fn placement_hint(f: &Face, w: f32, h: f32, tilt: bool) -> Option<Hint> {
    let [x, y, bw, bh] = f.bbox;
    let (cx, cy) = (x + bw / 2.0, y + bh / 2.0);
    let dy = if tilt { 0.26 } else { 0.18 };
    if (cx - w / 2.0).abs() > w * 0.18 || (cy - h * 0.45).abs() > h * dy {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn face(cx: f32, cy: f32, w: f32) -> Face {
        Face { bbox: [cx - w / 2.0, cy - w / 2.0, w, w * 1.2], score: 0.9, landmarks: [[0.0; 2]; 5] }
    }

    #[test]
    fn placement_checks() {
        let (w, h) = (480.0, 640.0);
        assert_eq!(placement_hint(&face(240.0, 290.0, 200.0), w, h, false), None);
        assert_eq!(placement_hint(&face(60.0, 290.0, 200.0), w, h, false), Some(Hint::NotCentered));
        assert_eq!(placement_hint(&face(240.0, 290.0, 60.0), w, h, false), Some(Hint::TooFar));
        assert_eq!(placement_hint(&face(240.0, 290.0, 420.0), w, h, false), Some(Hint::TooClose));
        // Iyak ko'tarilganda yuz yuqoriga siljiydi: tilt rejimida ruxsat
        assert_eq!(placement_hint(&face(240.0, 130.0, 200.0), w, h, false), Some(Hint::NotCentered));
        assert_eq!(placement_hint(&face(240.0, 130.0, 200.0), w, h, true), None);
    }

    #[test]
    fn directions_around_circle() {
        assert!(Pose::is_frontal(3.0, -4.0) && !Pose::is_frontal(20.0, 0.0));
        assert_eq!(Pose::direction(3.0, 2.0), None, "markazga juda yaqin");
        assert_eq!(Pose::direction(60.0, 0.0), None, "haddan tashqari burilgan");
        assert_eq!(Pose::direction(20.0, 1.0), Some(Pose::Left));
        assert_eq!(Pose::direction(-20.0, 1.0), Some(Pose::Right));
        assert_eq!(Pose::direction(1.0, 12.0), Some(Pose::Up));
        assert_eq!(Pose::direction(1.0, -12.0), Some(Pose::Down));
        assert_eq!(Pose::direction(15.0, 9.0), Some(Pose::UpLeft));
        assert_eq!(Pose::direction(-15.0, 9.0), Some(Pose::UpRight));
        assert_eq!(Pose::direction(-15.0, -9.0), Some(Pose::DownRight));
        assert_eq!(Pose::direction(15.0, -9.0), Some(Pose::DownLeft));
        // Diagonal: kamroq burilish ham yetadi (to'g'ri yo'nalish uchun bu hali markaz)
        assert_eq!(Pose::direction(9.0, 3.0), Some(Pose::UpLeft));
        assert_eq!(Pose::direction(9.0, 0.5), None);
        assert_eq!(Pose::direction(-30.0, 1.0), Some(Pose::Right), "to'g'ri yo'nalish o'zgarmadi");
    }

    fn sample_at(v: &[f32], pose: Option<Pose>, yaw_deg: f32, pitch_deg: f32) -> Sample {
        let mut v = v.to_vec();
        crate::face::matcher::l2_normalize(&mut v);
        Sample { embedding: v, thumb_png: vec![1, 2, 3], score: 0.9, pose, yaw_deg, pitch_deg }
    }

    fn sample(v: &[f32]) -> Sample {
        sample_at(v, Some(Pose::Frontal), 0.0, 0.0)
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
    fn target_follows_missing_poses() {
        let mut s = Session::default();
        assert_eq!(s.target(), Some(Pose::Frontal));
        s.samples.push(sample_at(&[1.0, 0.0], Some(Pose::Frontal), 0.0, 0.0));
        s.samples.push(sample_at(&[1.0, 0.0], Some(Pose::Left), 20.0, 0.0));
        s.samples.push(sample_at(&[1.0, 0.0], Some(Pose::Up), 0.0, 10.0));
        assert_eq!(s.target(), Some(Pose::UpLeft));
        assert_eq!(s.covered(), vec![Pose::Left, Pose::Up]);
        // Chap namuna o'chirilsa, u yana bo'sh sektor
        s.remove(1);
        assert_eq!(s.target(), Some(Pose::Left));
        assert_eq!(s.covered(), vec![Pose::Up]);
        s.samples = vec![sample(&[1.0, 0.0]); TARGET_SAMPLES];
        assert_eq!(s.target(), None);
    }

    #[test]
    fn poses_are_relative_to_frontal_sample() {
        // Bu odamda to'g'ri qaraganda ham pitch −8° chiqadi (landmark joylashuvi)
        let mut s = Session::default();
        s.samples.push(sample_at(&[1.0, 0.0], Some(Pose::Frontal), 2.0, -8.0));
        let (by, bp) = s.base();
        // Mutlaq +1° — chegaradan past, lekin nisbiy +9° — yuqoriga qaragan
        assert_eq!(Pose::direction(2.0, 1.0), None);
        assert_eq!(Pose::direction(2.0 - by, 1.0 - bp), Some(Pose::Up));
    }
}
