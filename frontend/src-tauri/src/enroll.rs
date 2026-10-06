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
/// Burilish chegarasi (to'g'ri qarashga nisbatan, gradus). Pitch landmarklardan kichikroq
/// chiqadi, shuning uchun uning ostonasi pastroq: (yaw/MIN_YAW, pitch/MIN_PITCH) ellips.
const MIN_YAW: f32 = 12.0;
const MIN_PITCH: f32 = 7.0;
const MAX_ANGLE: f32 = 45.0;
/// Diagonalda bosh ikki o'q bo'yicha bir vaqtda buriladi — qiyinroq, shuning uchun
/// kerakli masofa kichikroq (to'g'ri yo'nalishda 1, diagonalda shu qiymat, oralig'i silliq).
const DIAGONAL_K: f32 = 0.75;
/// Halqa: Face ID kabi 72 ta chiziqcha (5°). Har bir namuna o'z burchagidan ±COVER_HALF_DEG
/// atrofini qoplaydi; hammasi qoplanganda tugaydi — namunalar soni oldindan belgilanmaydi.
pub const BINS: usize = 72;
const BIN_DEG: f32 = 360.0 / BINS as f32;
const COVER_HALF_DEG: f32 = 20.0;

/// Namuna olingan yo'nalish (ko'rib chiqishdagi yorliq uchun): to'g'ri qarash yoki
/// bosh aylanasidagi 8 tomondan eng yaqini.
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

impl Pose {
    fn is_frontal(yaw: f32, pitch: f32) -> bool {
        yaw.abs() <= MIN_YAW && pitch.abs() <= 14.0
    }

    fn from_angle(deg: f32) -> Pose {
        DIRECTIONS[((deg / 45.0).round() as i64).rem_euclid(8) as usize]
    }
}

/// Nisbiy burchaklardan: aylanadagi burchak (0..360, 0° — chap) va chegaraga nisbatan
/// masofa (≥ 1 — yetarlicha burilgan). Haddan tashqari burilgan bo'lsa — `None`.
fn polar(yaw: f32, pitch: f32) -> Option<(f32, f32)> {
    if yaw.abs() > MAX_ANGLE || pitch.abs() > MAX_ANGLE {
        return None;
    }
    let (x, y) = (yaw / MIN_YAW, pitch / MIN_PITCH);
    let a = y.atan2(x).to_degrees().rem_euclid(360.0);
    let k = 1.0 - (1.0 - DIAGONAL_K) * (2.0 * a.to_radians()).sin().abs();
    Some((a, x.hypot(y) / k))
}

fn bin_of(deg: f32) -> usize {
    ((deg.rem_euclid(360.0) / BIN_DEG) as usize).min(BINS - 1)
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    pub count: usize,
    pub target: Option<Pose>,
    pub hint: Option<Hint>,
    /// Halqaning qoplangan qismlari: `BINS` ta, `i`-element — `[i·5°, (i+1)·5°)`.
    pub covered: Vec<bool>,
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
    /// Aylanadagi burchak (`polar`); to'g'ri qarash uchun — `None`.
    pub angle: Option<f32>,
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
    /// Mavjud shaxsga namuna qo'shilmoqda (shaxs sahifasidan); `None` — yangi shaxs.
    pub person_id: Option<i64>,
    pub samples: Vec<Sample>,
    /// Shu kadrdagi taxminiy burchaklar (live landmarklardan): sifat tekshiruvi o'tmasa ham
    /// ekrandagi yuz to'ri bosh burilishini ko'rsatib tursin.
    rough_pose: Option<(f32, f32)>,
    /// Ekrandagi doira (frontend o'lchaydi); bo'lmasa — standart joy.
    pub region: Option<Region>,
}

impl Session {
    fn has_frontal(&self) -> bool {
        self.samples.iter().any(|s| s.pose == Some(Pose::Frontal))
    }

    /// Namunalardan hisoblanadi: ko'rib chiqishda o'chirilgan namunaning qismi yana ochiladi.
    pub fn covered(&self) -> Vec<bool> {
        let mut c = vec![false; BINS];
        let span = (COVER_HALF_DEG / BIN_DEG).round() as i64;
        for a in self.samples.iter().filter_map(|s| s.angle) {
            let center = bin_of(a) as i64;
            for d in -span..=span {
                c[(center + d).rem_euclid(BINS as i64) as usize] = true;
            }
        }
        c
    }

    /// Avval to'g'ri qarash, keyin halqaning qoplanmagan qismi; hammasi qoplansa — `None`.
    pub fn target(&self) -> Option<Pose> {
        if !self.has_frontal() {
            return Some(Pose::Frontal);
        }
        let i = self.covered().iter().position(|&c| !c)?;
        Some(Pose::from_angle((i as f32 + 0.5) * BIN_DEG))
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
            yaw_deg: q.map(|q| q.yaw_deg).or(self.rough_pose.map(|p| p.0)).map(|v| v - by),
            pitch_deg: q.map(|q| q.pitch_deg).or(self.rough_pose.map(|p| p.1)).map(|v| v - bp),
            sharpness: q.map(|q| q.sharpness),
        }
    }

    /// Kamera kadri. `faces` — live detektor natijasi (160 px, taxminiy).
    pub fn process_frame(&mut self, engine: &FaceEngine, img: &RgbImage, faces: &[Face]) -> Feedback {
        if self.target().is_none() {
            return self.feedback(None, false, None);
        }
        let reject = |s: &mut Self, h: Hint, q: Option<&quality::Quality>| s.feedback(Some(h), false, q);
        self.rough_pose = None;
        let face = match faces {
            [] => return reject(self, Hint::NoFace, None),
            [f] => f,
            _ => return reject(self, Hint::MultipleFaces, None),
        };
        self.rough_pose = Some(quality::estimate_pose(&face.landmarks));
        // Bosh aylanayotganda yuz ovaldan biroz chiqadi — vertikal chegara kengroq.
        let around = self.has_frontal();
        if let Some(h) = placement_hint(face, img.width() as f32, img.height() as f32, around, self.region.unwrap_or_default()) {
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
        // Kutmasdan, birinchi sifatli kadrda olinadi: xira kadrni quality_hint o'zi rad etadi.
        let (pose, angle) = if around {
            match polar(yaw, pitch) {
                Some((a, r)) if r >= 1.0 && !self.covered()[bin_of(a)] => (Pose::from_angle(a), Some(a)),
                _ => return reject(self, Hint::TurnPose, Some(&q)),
            }
        } else if Pose::is_frontal(yaw, pitch) {
            (Pose::Frontal, None)
        } else {
            return reject(self, Hint::TurnPose, Some(&q));
        };
        match self.capture(engine, &face, aligned, Some(pose), angle, &q) {
            Ok(true) => self.feedback(None, true, Some(&q)),
            Ok(false) => reject(self, Hint::DifferentPerson, Some(&q)),
            Err(e) => {
                log::warn!("namuna olinmadi: {e}");
                reject(self, Hint::LowScore, Some(&q))
            }
        }
    }

    /// `false` — namuna oldingilariga o'xshamaydi (boshqa odam).
    fn capture(
        &mut self,
        engine: &FaceEngine,
        face: &Face,
        aligned: RgbImage,
        pose: Option<Pose>,
        angle: Option<f32>,
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
            angle,
            yaw_deg: q.yaw_deg,
            pitch_deg: q.pitch_deg,
        });
        Ok(true)
    }

    /// Bazadagi eng o'xshash shaxs (chegaradan o'tsa). Barcha namunalar bo'yicha eng yuqori ball.
    /// Mavjud shaxsga qo'shilayotganda uning o'zi dublikat hisoblanmaydi.
    pub fn duplicate(&self, matcher: &Matcher, threshold: f32) -> Option<(i64, f32)> {
        self.samples
            .iter()
            .filter_map(|s| matcher.nearest(&s.embedding))
            .filter(|m| m.score >= threshold && Some(m.person_id) != self.person_id)
            .max_by(|a, b| a.score.total_cmp(&b.score))
            .map(|m| (m.person_id, m.score))
    }

    /// Shaxs va namunalar bitta tranzaksiyada saqlanadi, keyin Matcher yangilanadi.
    /// `person_id` bo'lsa, namunalar o'sha shaxsga qo'shiladi (`input` e'tiborsiz).
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
        let person = match self.person_id {
            Some(id) => {
                repo::add_embeddings(conn, id, &rows)?;
                repo::get_person(conn, id)?
            }
            None => repo::create_person(conn, input, &rows)?,
        };
        // Faolsizlantirilgan shaxs tanilmaydi: u faollashtirilganda Matcher bazadan qayta yuklanadi.
        if person.is_active {
            for s in &self.samples {
                matcher.add(person.id, &s.embedding);
            }
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

/// Ekrandagi tiniq doiraning kadrdagi joyi: markaz — kadr kengligi/balandligi ulushida,
/// radius — kadr kengligi ulushida. Doira o'lchami telefon ekraniga bog'liq, shuning uchun
/// frontend uni hisoblab yuboradi (`enroll_set_region`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct Region {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
}

impl Default for Region {
    fn default() -> Self {
        Self { cx: 0.5, cy: 0.45, r: 0.3 }
    }
}

/// Yuz doira ichida bo'lishi kerak: markazi doira markaziga yaqin, kattaligi doiraga mos.
/// `tilt` — bosh aylanayotgan bosqich: burilganda yuz ramkasi siljiydi, chegara kengroq.
fn placement_hint(f: &Face, w: f32, h: f32, tilt: bool, region: Region) -> Option<Hint> {
    let [x, y, bw, _] = f.bbox;
    let rc = region.r * w;
    let (dx, dy) = (x + bw / 2.0 - region.cx * w, y + f.bbox[3] / 2.0 - region.cy * h);
    if dx.hypot(dy) > rc * if tilt { 0.5 } else { 0.35 } {
        return Some(Hint::NotCentered);
    }
    if bw < rc * 0.9 {
        return Some(Hint::TooFar);
    }
    if bw > rc * 2.1 {
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
        let r = Region::default(); // markaz (240, 288), radius 144 px
        assert_eq!(placement_hint(&face(240.0, 290.0, 200.0), w, h, false, r), None);
        assert_eq!(placement_hint(&face(60.0, 290.0, 200.0), w, h, false, r), Some(Hint::NotCentered));
        assert_eq!(placement_hint(&face(240.0, 290.0, 60.0), w, h, false, r), Some(Hint::TooFar));
        assert_eq!(placement_hint(&face(240.0, 290.0, 420.0), w, h, false, r), Some(Hint::TooClose));
        // Bosh aylanayotganda yuz ramkasi siljiydi: tilt rejimida kengroq
        assert_eq!(placement_hint(&face(240.0, 215.0, 200.0), w, h, false, r), Some(Hint::NotCentered));
        assert_eq!(placement_hint(&face(240.0, 215.0, 200.0), w, h, true, r), None);
        // Ekrandagi doira boshqa joyda va kichikroq bo'lsa — shunga nisbatan
        let small = Region { cx: 0.5, cy: 0.35, r: 0.2 }; // markaz (240, 224), radius 96 px
        assert_eq!(placement_hint(&face(240.0, 290.0, 200.0), w, h, false, small), Some(Hint::NotCentered));
        assert_eq!(placement_hint(&face(240.0, 226.0, 150.0), w, h, false, small), None);
        assert_eq!(placement_hint(&face(240.0, 226.0, 220.0), w, h, false, small), Some(Hint::TooClose));
    }

    #[test]
    fn polar_thresholds() {
        assert!(Pose::is_frontal(3.0, -4.0) && !Pose::is_frontal(20.0, 0.0));
        assert_eq!(polar(60.0, 0.0), None, "haddan tashqari burilgan");
        let r = |y, p| polar(y, p).unwrap().1;
        assert!(r(3.0, 2.0) < 1.0, "markazga juda yaqin");
        assert!(r(12.5, 0.0) >= 1.0 && r(-12.5, 0.0) >= 1.0, "chap/o'ng ~12°");
        assert!(r(0.0, 7.5) >= 1.0 && r(0.0, -7.5) >= 1.0, "yuqori/past ~7°");
        // Diagonal: kamroq burilish ham yetadi
        assert!(r(7.0, 4.2) >= 1.0 && r(7.0, 4.2) < 1.2);
        assert!(r(9.0, 0.5) < 1.0);
        // Burchak: 0° — chap, 90° — yuqori, 180° — o'ng
        let a = |y, p| Pose::from_angle(polar(y, p).unwrap().0);
        assert_eq!(a(20.0, 0.0), Pose::Left);
        assert_eq!(a(0.0, 10.0), Pose::Up);
        assert_eq!(a(-20.0, 0.0), Pose::Right);
        assert_eq!(a(0.0, -10.0), Pose::Down);
        assert_eq!(a(-10.0, -6.0), Pose::DownRight);
    }

    fn sample_at(v: &[f32], pose: Option<Pose>, yaw_deg: f32, pitch_deg: f32) -> Sample {
        let mut v = v.to_vec();
        crate::face::matcher::l2_normalize(&mut v);
        Sample { embedding: v, thumb_png: vec![1, 2, 3], score: 0.9, pose, angle: None, yaw_deg, pitch_deg }
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

        // Mavjud shaxsga qo'shish: o'zi dublikat emas, namunalar unga qo'shiladi
        let more = Session { person_id: Some(p.id), samples: vec![sample(&[0.98, 0.1])], ..Default::default() };
        assert_eq!(more.duplicate(&matcher, 0.36), None);
        let p2 = more.commit(&mut conn, &mut matcher, &PersonInput::default()).unwrap();
        assert_eq!((p2.id, p2.sample_count), (p.id, 3));
        assert_eq!(matcher.len(), 3);
        assert_eq!(repo::list_persons(&conn, None, true).unwrap().len(), 1);
    }

    fn at_angle(deg: f32) -> Sample {
        let mut s = sample_at(&[1.0, 0.0], Some(Pose::from_angle(deg)), 0.0, 0.0);
        s.angle = Some(deg);
        s
    }

    #[test]
    fn coverage_completes_ring_with_any_count() {
        let mut s = Session::default();
        assert_eq!(s.target(), Some(Pose::Frontal));
        s.samples.push(sample(&[1.0, 0.0]));
        assert!(s.covered().iter().all(|&c| !c), "to'g'ri qarash halqani qoplamaydi");
        // Bitta namuna ±20° ni qoplaydi: 0° atrofida 355°..20°
        s.samples.push(at_angle(0.0));
        let c = s.covered();
        assert!(c[0] && c[4] && c[BINS - 4]);
        assert!(!c[5] && !c[BINS - 5]);
        assert_eq!(s.target(), Some(Pose::from_angle(27.5)));
        // 40° qadam bilan 9 ta namuna — halqa to'ldi, namunalar soni muhim emas
        for i in 1..9 {
            s.samples.push(at_angle(i as f32 * 40.0));
        }
        assert!(s.covered().iter().all(|&c| c));
        assert_eq!(s.target(), None);
        // Ko'rib chiqishda namuna o'chirilsa, uning qismi yana ochiladi
        s.remove(3);
        assert!(s.target().is_some());
        assert!(!s.covered()[bin_of(80.0)]);
    }

    #[test]
    fn poses_are_relative_to_frontal_sample() {
        // Bu odamda to'g'ri qaraganda ham pitch −8° chiqadi (landmark joylashuvi)
        let mut s = Session::default();
        s.samples.push(sample_at(&[1.0, 0.0], Some(Pose::Frontal), 2.0, -8.0));
        let (by, bp) = s.base();
        // Mutlaq 0° — chegaradan past, lekin nisbiy +8° — yuqoriga qaragan
        assert!(polar(2.0, 0.0).unwrap().1 < 1.0);
        let (a, r) = polar(2.0 - by, 0.0 - bp).unwrap();
        assert!(r >= 1.0);
        assert_eq!(Pose::from_angle(a), Pose::Up);
    }
}
