//! Administrator rejimi: PIN (Argon2id), vaqtinchalik sessiya, noto'g'ri urinishlarda
//! bloklash va admin jurnali.
//!
//! Sessiya faqat xotirada turadi: ilova qayta ochilsa, doim mehmon rejimi. Muddat
//! `CLOCK_BOOTTIME` bo'yicha hisoblanadi — telefon uxlab yotgan vaqt ham kiradi
//! (`Instant` Android'da uyquda to'xtab qoladi va sessiya ochiq qolib ketardi).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use argon2::{Algorithm, Argon2, Params, Version};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Sessiya holati o'zgarganda (ochildi, uzaytirildi, yopildi) frontendga yuboriladi.
pub const SESSION_EVENT: &str = "admin://session";
pub const PIN_LEN: usize = 6;
pub const SESSION_KEY: &str = "security.session_min";
pub const DEFAULT_SESSION_MIN: u32 = 3;
pub const SESSION_MIN_RANGE: std::ops::RangeInclusive<u32> = 1..=10;
/// `admin.` bilan boshlanadigan sozlamalar frontendga berilmaydi va `set_setting` bilan yozilmaydi.
pub const PRIVATE_PREFIX: &str = "admin.";
const PIN_KEY: &str = "admin.pin_hash";
const FAILED_KEY: &str = "admin.failed";
const LOCKED_KEY: &str = "admin.locked_until";
/// Shuncha xatogacha blok yo'q.
const FREE_ATTEMPTS: u32 = 4;

/// OWASP tavsiyasi (19 MiB, 2 o'tish): Redmi 5 Plus'da ~0,5 s.
const M_COST: u32 = 19 * 1024;
const T_COST: u32 = 2;
const P_COST: u32 = 1;
const HASH_LEN: usize = 32;

/// Monoton soat (ms), telefon uyqusidagi vaqt bilan birga.
pub fn now_ms() -> u64 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: `ts` — yaroqli ko'rsatkich; CLOCK_BOOTTIME Linux 2.6.39+ da bor.
        if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) } == 0 {
            return ts.tv_sec as u64 * 1000 + ts.tv_nsec as u64 / 1_000_000;
        }
    }
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------- Sessiya ----------

pub struct Admin {
    /// Sessiya tugash vaqti (`now_ms` shkalasida); `None` — mehmon rejimi.
    deadline: Mutex<Option<u64>>,
    timeout_ms: AtomicU64,
    /// PIN tekshiruvlari ketma-ket: parallel so'rovlar bilan bloklashni aylanib o'tib bo'lmaydi.
    verify: Mutex<()>,
}

/// Sessiya yo'q: `expired` — hozirgina muddati o'tdi (yopilishini e'lon qilish kerak).
struct Closed {
    expired: bool,
}

impl Admin {
    pub fn new(session_min: u32) -> Self {
        let a = Self { deadline: Mutex::new(None), timeout_ms: AtomicU64::new(0), verify: Mutex::new(()) };
        a.set_timeout_min(session_min);
        a
    }

    pub fn set_timeout_min(&self, min: u32) {
        let min = min.clamp(*SESSION_MIN_RANGE.start(), *SESSION_MIN_RANGE.end());
        self.timeout_ms.store(min as u64 * 60_000, Ordering::Relaxed);
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms.load(Ordering::Relaxed)
    }

    fn open(&self, now: u64) {
        *self.deadline.lock() = Some(now + self.timeout_ms());
    }

    /// Qolgan vaqt (ms). `extend` bo'lsa, muddat yana to'liq uzaytiriladi.
    fn remaining(&self, now: u64, extend: bool) -> Result<u64, Closed> {
        let mut d = self.deadline.lock();
        match *d {
            Some(end) if end > now => {
                if extend {
                    *d = Some(now + self.timeout_ms());
                    Ok(self.timeout_ms())
                } else {
                    Ok(end - now)
                }
            }
            Some(_) => {
                *d = None;
                Err(Closed { expired: true })
            }
            None => Err(Closed { expired: false }),
        }
    }

    /// Ochiq bo'lgan bo'lsa, `true`.
    fn close(&self) -> bool {
        self.deadline.lock().take().is_some()
    }
}

fn admin(app: &AppHandle) -> &Admin {
    &app.state::<AppState>().inner().admin
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionEvent {
    active: bool,
    remaining_ms: u64,
    timeout_ms: u64,
    /// Yopilish sababi: "logout" | "expire" | "reset".
    reason: Option<&'static str>,
}

fn emit(app: &AppHandle, remaining_ms: u64, reason: Option<&'static str>) {
    let ev = SessionEvent { active: remaining_ms > 0, remaining_ms, timeout_ms: admin(app).timeout_ms(), reason };
    let _ = app.emit(SESSION_EVENT, ev);
}

fn closed(app: &AppHandle, c: Closed) -> AppError {
    if c.expired {
        on_closed(app, "expire");
    }
    AppError::Forbidden
}

/// Admin sessiyasi ochiqligini tekshiradi (muddatni uzaytirmaydi): ro'yxat va holatni
/// o'qish kabi passiv so'rovlar uchun.
pub fn check(app: &AppHandle) -> AppResult<()> {
    admin(app).remaining(now_ms(), false).map(drop).map_err(|c| closed(app, c))
}

/// Admin amali: tekshiradi va muddatni yana to'liq uzaytiradi.
pub fn act(app: &AppHandle) -> AppResult<()> {
    let left = admin(app).remaining(now_ms(), true).map_err(|c| closed(app, c))?;
    emit(app, left, None);
    Ok(())
}

/// Kamera oqimidan: yuz qo'shishda namuna olinganda sessiya uzayadi.
pub fn extend_if_active(app: &AppHandle) {
    if let Ok(left) = admin(app).remaining(now_ms(), true) {
        emit(app, left, None);
    }
}

fn open_session(app: &AppHandle) {
    admin(app).open(now_ms());
    emit(app, admin(app).timeout_ms(), None);
    apply_secure(app);
}

pub fn close_session(app: &AppHandle, reason: &'static str) {
    if admin(app).close() {
        on_closed(app, reason);
    }
}

fn on_closed(app: &AppHandle, reason: &'static str) {
    let state = app.state::<AppState>();
    // Yarim qolgan yuz qo'shish bekor qilinadi: namunalar xotirada qolmasin.
    *state.enroll.lock() = None;
    if reason != "reset" {
        let _ = log_event(&state.db.lock(), reason, None);
    }
    emit(app, 0, Some(reason));
    apply_secure(app);
}

/// Muddat o'tganini fonda kuzatadi: frontend hech narsa so'ramasa ham sessiya yopiladi.
/// Telefon uxlaganda oqim ham uxlaydi, lekin uyg'onishi bilan (BOOTTIME bo'yicha) yopadi.
pub fn spawn_watcher(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("admin-session".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if let Err(c) = admin(&app).remaining(now_ms(), false) {
            if c.expired {
                on_closed(&app, "expire");
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("admin kuzatuvchisi ishga tushmadi: {e}");
    }
}

/// FLAG_SECURE: admin rejimida doim yoqiq ("so'nggi ilovalar"da shaxslar ro'yxati
/// ko'rinmasin), mehmon rejimida — sozlamaga ko'ra.
pub fn apply_secure(app: &AppHandle) {
    let app = app.clone();
    // Holat shu yerda o'qiladi: ketma-ket chaqiruvlarda oxirgisi to'g'ri qiymatni qo'yadi.
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_khoface_camera::CameraExt;
        let state = app.state::<AppState>();
        let active = state.admin.remaining(now_ms(), false).is_ok();
        let setting = repo::get_setting(&state.db.lock(), "privacy.secure_screen").ok().flatten();
        if let Err(e) = app.camera().set_secure(active || setting.as_deref() == Some("1")) {
            log::warn!("FLAG_SECURE: {e}");
        }
    });
}

// ---------- PIN ----------

/// 6 raqam; `111111`, `123456`, `987654` kabi ketma-ketliklar qabul qilinmaydi.
pub fn validate_pin(pin: &str) -> AppResult<()> {
    if pin.len() != PIN_LEN || !pin.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::Invalid(format!("PIN {PIN_LEN} ta raqamdan iborat bo'lishi kerak")));
    }
    let d: Vec<i32> = pin.bytes().map(|b| (b - b'0') as i32).collect();
    let step = d[1] - d[0];
    if step.abs() <= 1 && d.windows(2).all(|w| w[1] - w[0] == step) {
        return Err(AppError::Invalid("PIN juda oddiy".into()));
    }
    Ok(())
}

fn derive(pin: &str, salt: &[u8], m: u32, t: u32, p: u32) -> AppResult<[u8; HASH_LEN]> {
    let params = Params::new(m, t, p, Some(HASH_LEN)).map_err(|e| AppError::Invalid(format!("argon2: {e}")))?;
    let mut out = [0u8; HASH_LEN];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut out)
        .map_err(|e| AppError::Invalid(format!("argon2: {e}")))?;
    Ok(out)
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// `argon2id$<m>$<t>$<p>$<salt hex>$<hash hex>` — parametrlar saqlanadi, keyin
/// oshirilsa ham eski PIN ishlayveradi.
pub fn hash_pin(pin: &str) -> AppResult<String> {
    // UUID v4 — OS tasodifiy manbasidan 122 bit; alohida RNG kutubxonasi shart emas.
    let salt = *uuid::Uuid::new_v4().as_bytes();
    let hash = derive(pin, &salt, M_COST, T_COST, P_COST)?;
    Ok(format!("argon2id${M_COST}${T_COST}${P_COST}${}${}", to_hex(&salt), to_hex(&hash)))
}

pub fn verify_pin(pin: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    let [alg, m, t, p, salt, hash] = parts[..] else { return false };
    let (Ok(m), Ok(t), Ok(p), Some(salt), Some(hash)) = (m.parse(), t.parse(), p.parse(), from_hex(salt), from_hex(hash)) else {
        return false;
    };
    if alg != "argon2id" || hash.len() != HASH_LEN {
        return false;
    }
    let Ok(got) = derive(pin, &salt, m, t, p) else { return false };
    // Doimiy vaqtli solishtirish.
    got.iter().zip(&hash).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Ketma-ket xatolar soniga ko'ra blok (soniya): 5 → 30 s, 6 → 1 daq, 7 → 5 daq, keyin 15 daq.
pub fn lock_secs(failed: u32) -> u64 {
    match failed {
        0..=FREE_ATTEMPTS => 0,
        5 => 30,
        6 => 60,
        7 => 300,
        _ => 900,
    }
}

struct Attempts {
    failed: u32,
    locked_until: u64,
}

impl Attempts {
    fn read(c: &Connection) -> AppResult<Self> {
        let num = |k| -> AppResult<u64> { Ok(repo::get_setting(c, k)?.and_then(|v| v.parse().ok()).unwrap_or(0)) };
        Ok(Self { failed: num(FAILED_KEY)? as u32, locked_until: num(LOCKED_KEY)? })
    }

    fn write(&self, c: &Connection) -> AppResult<()> {
        repo::set_setting(c, FAILED_KEY, &self.failed.to_string())?;
        repo::set_setting(c, LOCKED_KEY, &self.locked_until.to_string())
    }

    /// Qolgan blok (s). Soat orqaga surilsa ham to'liq blok muddatidan oshmaydi.
    fn locked_for(&self, now: u64) -> u64 {
        self.locked_until.saturating_sub(now).min(lock_secs(self.failed))
    }
}

pub fn is_configured(c: &Connection) -> AppResult<bool> {
    Ok(repo::get_setting(c, PIN_KEY)?.is_some())
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Verdict {
    Ok,
    /// `attemptsLeft` — blokkacha qolgan urinishlar; `lockedMs` > 0 — endi bloklandi.
    Wrong { attempts_left: u32, locked_ms: u64 },
    Locked { locked_ms: u64 },
    NotConfigured,
}

/// PIN'ni tekshiradi va urinishlar hisobini yuritadi. Argon2 bazani band qilmasdan
/// hisoblanadi (aks holda jonli tanish yarim soniya qotib qolardi).
fn attempt(state: &AppState, pin: &str) -> AppResult<Verdict> {
    let _serial = state.admin.verify.lock();
    let now = unix_now();
    let (stored, mut a) = {
        let c = state.db.lock();
        let Some(stored) = repo::get_setting(&c, PIN_KEY)? else { return Ok(Verdict::NotConfigured) };
        (stored, Attempts::read(&c)?)
    };
    let left = a.locked_for(now);
    if left > 0 {
        return Ok(Verdict::Locked { locked_ms: left * 1000 });
    }
    let ok = verify_pin(pin, &stored);
    let c = state.db.lock();
    if ok {
        Attempts { failed: 0, locked_until: 0 }.write(&c)?;
        return Ok(Verdict::Ok);
    }
    a.failed += 1;
    let lock = lock_secs(a.failed);
    a.locked_until = now + lock;
    a.write(&c)?;
    log_event(&c, "fail", None)?;
    if lock > 0 {
        log_event(&c, "lockout", Some(&format!("{lock} s")))?;
    }
    Ok(Verdict::Wrong { attempts_left: (FREE_ATTEMPTS + 1).saturating_sub(a.failed), locked_ms: lock * 1000 })
}

/// Birinchi ishga tushirishda PIN o'rnatadi va sessiyani ochadi.
pub fn setup(app: &AppHandle, pin: &str) -> AppResult<()> {
    validate_pin(pin)?;
    let hash = hash_pin(pin)?;
    {
        let state = app.state::<AppState>();
        let c = state.db.lock();
        if is_configured(&c)? {
            return Err(AppError::Invalid("PIN allaqachon o'rnatilgan".into()));
        }
        repo::set_setting(&c, PIN_KEY, &hash)?;
        Attempts { failed: 0, locked_until: 0 }.write(&c)?;
        log_event(&c, "setup", None)?;
    }
    open_session(app);
    Ok(())
}

pub fn unlock(app: &AppHandle, pin: &str) -> AppResult<Verdict> {
    let v = attempt(&app.state::<AppState>(), pin)?;
    if v == Verdict::Ok {
        let _ = log_event(&app.state::<AppState>().db.lock(), "unlock", None);
        open_session(app);
    }
    Ok(v)
}

/// Eski PIN noto'g'ri bo'lsa — oddiy xato urinish kabi hisoblanadi.
pub fn change_pin(app: &AppHandle, old: &str, new: &str) -> AppResult<Verdict> {
    act(app)?;
    validate_pin(new)?;
    let v = attempt(&app.state::<AppState>(), old)?;
    if v == Verdict::Ok {
        let hash = hash_pin(new)?;
        let state = app.state::<AppState>();
        let c = state.db.lock();
        repo::set_setting(&c, PIN_KEY, &hash)?;
        log_event(&c, "pin_change", None)?;
    }
    Ok(v)
}

/// PIN unutilganda: barcha biometrik ma'lumot, PIN va admin jurnali o'chiriladi.
/// Ilovaning "Ma'lumotlarni tozalash" tugmasi bilan bir xil natija — yangi imkoniyat bermaydi.
pub fn reset_all(c: &mut Connection) -> AppResult<()> {
    c.execute("DELETE FROM settings WHERE key LIKE 'admin.%'", [])?;
    c.execute("DELETE FROM admin_events", [])?;
    repo::wipe_all(c)?;
    log_event(c, "reset", None)?;
    Ok(())
}

/// Zaxiradan tiklashda joriy PIN saqlanib qoladi (eski zaxiradagi PIN qaytib kelmasin).
pub fn private_settings(c: &Connection) -> AppResult<Vec<(String, String)>> {
    Ok(repo::get_settings(c)?.into_iter().filter(|(k, _)| k.starts_with(PRIVATE_PREFIX)).collect())
}

pub fn put_private_settings(c: &Connection, rows: &[(String, String)]) -> AppResult<()> {
    c.execute("DELETE FROM settings WHERE key LIKE 'admin.%'", [])?;
    for (k, v) in rows {
        repo::set_setting(c, k, v)?;
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    active: bool,
    remaining_ms: u64,
    timeout_ms: u64,
    /// PIN kiritish bloklangan bo'lsa — qolgan vaqt.
    locked_ms: u64,
}

pub fn status(app: &AppHandle) -> AppResult<Status> {
    let remaining_ms = match admin(app).remaining(now_ms(), false) {
        Ok(ms) => ms,
        Err(c) => {
            if c.expired {
                on_closed(app, "expire");
            }
            0
        }
    };
    let state = app.state::<AppState>();
    let c = state.db.lock();
    Ok(Status {
        configured: is_configured(&c)?,
        active: remaining_ms > 0,
        remaining_ms,
        timeout_ms: state.admin.timeout_ms(),
        locked_ms: Attempts::read(&c)?.locked_for(unix_now()) * 1000,
    })
}

// ---------- Jurnal ----------

pub fn log_event(c: &Connection, kind: &str, detail: Option<&str>) -> AppResult<()> {
    c.execute("INSERT INTO admin_events (kind, detail) VALUES (?1, ?2)", params![kind, detail])?;
    Ok(())
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdminEvent {
    id: i64,
    kind: String,
    detail: Option<String>,
    created_at: String,
}

pub fn list_events(c: &Connection, limit: u32) -> AppResult<Vec<AdminEvent>> {
    let mut st = c.prepare_cached("SELECT id, kind, detail, created_at FROM admin_events ORDER BY id DESC LIMIT ?1")?;
    let rows = st.query_map([limit.min(500)], |r| {
        Ok(AdminEvent { id: r.get(0)?, kind: r.get(1)?, detail: r.get(2)?, created_at: r.get(3)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn pin_rules() {
        for ok in ["428193", "000100", "135790"] {
            assert!(validate_pin(ok).is_ok(), "{ok}");
        }
        for bad in ["12345", "1234567", "12a456", "111111", "123456", "654321", "", "١٢٣٤٥٦"] {
            assert!(validate_pin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn hash_roundtrip() {
        let h = hash_pin("428193").unwrap();
        assert!(verify_pin("428193", &h));
        assert!(!verify_pin("428194", &h));
        // Tuz har safar boshqa
        assert_ne!(h, hash_pin("428193").unwrap());
        assert!(!verify_pin("428193", "argon2id$1$1$1$zz$zz"));
        assert!(!verify_pin("428193", ""));
    }

    #[test]
    fn verdict_json_matches_frontend() {
        let j = |v: Verdict| serde_json::to_string(&v).unwrap();
        assert_eq!(j(Verdict::Ok), r#"{"result":"ok"}"#);
        assert_eq!(j(Verdict::Wrong { attempts_left: 2, locked_ms: 0 }), r#"{"result":"wrong","attemptsLeft":2,"lockedMs":0}"#);
        assert_eq!(j(Verdict::Locked { locked_ms: 5 }), r#"{"result":"locked","lockedMs":5}"#);
        assert_eq!(j(Verdict::NotConfigured), r#"{"result":"notConfigured"}"#);
    }

    #[test]
    fn lockout_schedule() {
        assert_eq!((0..=4).map(lock_secs).sum::<u64>(), 0);
        assert_eq!(lock_secs(5), 30);
        assert_eq!(lock_secs(7), 300);
        assert_eq!(lock_secs(50), 900);
        // Soat orqaga surilsa, blok to'liq muddatdan uzun bo'lmaydi
        let a = Attempts { failed: 5, locked_until: 10_000 };
        assert_eq!(a.locked_for(100), 30);
        assert_eq!(a.locked_for(9_990), 10);
        assert_eq!(a.locked_for(10_001), 0);
    }

    #[test]
    fn attempts_lock_after_five_failures() {
        let state = AppState::new(open_in_memory().unwrap(), std::env::temp_dir());
        assert_eq!(attempt(&state, "428193").unwrap(), Verdict::NotConfigured);
        repo::set_setting(&state.db.lock(), PIN_KEY, &hash_pin("428193").unwrap()).unwrap();
        for left in (1..=4).rev() {
            assert_eq!(attempt(&state, "000100").unwrap(), Verdict::Wrong { attempts_left: left, locked_ms: 0 });
        }
        assert_eq!(attempt(&state, "000100").unwrap(), Verdict::Wrong { attempts_left: 0, locked_ms: 30_000 });
        // Blok vaqtida to'g'ri PIN ham qabul qilinmaydi
        assert!(matches!(attempt(&state, "428193").unwrap(), Verdict::Locked { .. }));
        let events = list_events(&state.db.lock(), 10).unwrap();
        assert_eq!(events.iter().filter(|e| e.kind == "fail").count(), 5);
        assert_eq!(events[0].kind, "lockout");
        // Blok tugagach to'g'ri PIN hisobni nolga tushiradi
        repo::set_setting(&state.db.lock(), LOCKED_KEY, "0").unwrap();
        assert_eq!(attempt(&state, "428193").unwrap(), Verdict::Ok);
        assert_eq!(Attempts::read(&state.db.lock()).unwrap().failed, 0);
    }

    #[test]
    fn session_expires_and_extends() {
        let a = Admin::new(3);
        assert!(a.remaining(0, false).is_err());
        a.open(1_000);
        assert_eq!(a.remaining(61_000, false).ok(), Some(120_000));
        // Amal muddatni yana 3 daqiqaga suradi
        assert_eq!(a.remaining(61_000, true).ok(), Some(180_000));
        assert!(a.remaining(240_999, false).is_ok());
        let c = a.remaining(241_000, false).unwrap_err();
        assert!(c.expired);
        // Yopilish bir marta e'lon qilinadi
        assert!(!a.remaining(241_001, false).unwrap_err().expired);
        assert!(!a.close());
    }

    #[test]
    fn timeout_is_clamped() {
        let a = Admin::new(0);
        assert_eq!(a.timeout_ms(), 60_000);
        a.set_timeout_min(99);
        assert_eq!(a.timeout_ms(), 600_000);
    }

    #[test]
    fn reset_wipes_pin_and_journal() {
        let mut c = open_in_memory().unwrap();
        repo::set_setting(&c, PIN_KEY, "x").unwrap();
        repo::set_setting(&c, "match.threshold", "0.4").unwrap();
        log_event(&c, "unlock", None).unwrap();
        reset_all(&mut c).unwrap();
        assert!(!is_configured(&c).unwrap());
        assert_eq!(repo::get_setting(&c, "match.threshold").unwrap().as_deref(), Some("0.4"));
        let ev = list_events(&c, 10).unwrap();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, "reset");
    }

    #[test]
    fn private_settings_survive_replace() {
        let c = open_in_memory().unwrap();
        repo::set_setting(&c, PIN_KEY, "new").unwrap();
        let saved = private_settings(&c).unwrap();
        repo::set_setting(&c, PIN_KEY, "old").unwrap();
        repo::set_setting(&c, FAILED_KEY, "3").unwrap();
        put_private_settings(&c, &saved).unwrap();
        assert_eq!(repo::get_setting(&c, PIN_KEY).unwrap().as_deref(), Some("new"));
        assert_eq!(repo::get_setting(&c, FAILED_KEY).unwrap(), None);
    }
}
