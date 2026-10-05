//! Barcha SQL so'rovlar shu yerda. Funksiyalar `&Connection` oladi, shuning uchun
//! testlarda in-memory baza bilan, ilovada esa `AppState.db` bilan ishlaydi.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{blob_to_vec, vec_to_blob};
use crate::error::{AppError, AppResult};

// ---------- Shaxslar ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: i64,
    pub uuid: String,
    pub full_name: String,
    pub external_id: Option<String>,
    pub note: Option<String>,
    pub consent_at: Option<String>,
    pub is_active: bool,
    pub sample_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonInput {
    pub full_name: String,
    pub external_id: Option<String>,
    pub note: Option<String>,
    pub consent_at: Option<String>,
}

impl PersonInput {
    fn validated(&self) -> AppResult<(String, Option<String>, Option<String>)> {
        let name = self.full_name.trim();
        if name.is_empty() {
            return Err(AppError::Invalid("ism bo'sh".into()));
        }
        let clean = |s: &Option<String>| {
            s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
        };
        Ok((name.to_string(), clean(&self.external_id), clean(&self.note)))
    }
}

const PERSON_COLS: &str = "p.id, p.uuid, p.full_name, p.external_id, p.note, p.consent_at, p.is_active,
     (SELECT COUNT(*) FROM face_embeddings e WHERE e.person_id = p.id), p.created_at, p.updated_at";

fn person_from_row(r: &Row) -> rusqlite::Result<Person> {
    Ok(Person {
        id: r.get(0)?,
        uuid: r.get(1)?,
        full_name: r.get(2)?,
        external_id: r.get(3)?,
        note: r.get(4)?,
        consent_at: r.get(5)?,
        is_active: r.get(6)?,
        sample_count: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

pub fn list_persons(conn: &Connection, query: Option<&str>, include_inactive: bool) -> AppResult<Vec<Person>> {
    let pattern = query
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    let sql = format!(
        "SELECT {PERSON_COLS} FROM persons p
         WHERE (?1 IS NULL OR p.full_name LIKE ?1 ESCAPE '\\' OR p.external_id LIKE ?1 ESCAPE '\\')
           AND (?2 OR p.is_active = 1)
         ORDER BY p.full_name COLLATE NOCASE"
    );
    let mut st = conn.prepare_cached(&sql)?;
    let rows = st.query_map(params![pattern, include_inactive], person_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_person(conn: &Connection, id: i64) -> AppResult<Person> {
    let sql = format!("SELECT {PERSON_COLS} FROM persons p WHERE p.id = ?1");
    conn.query_row(&sql, [id], person_from_row)
        .optional()?
        .ok_or(AppError::NotFound("shaxs"))
}

/// Shaxs va uning namunalarini bitta tranzaksiyada saqlaydi (ro'yxatga olish ustasining oxiri).
pub fn create_person(conn: &mut Connection, input: &PersonInput, samples: &[NewEmbedding]) -> AppResult<Person> {
    let (name, external_id, note) = input.validated()?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO persons (uuid, full_name, external_id, note, consent_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![uuid::Uuid::new_v4().to_string(), name, external_id, note, input.consent_at],
    )?;
    let id = tx.last_insert_rowid();
    insert_embeddings(&tx, id, samples)?;
    tx.commit()?;
    get_person(conn, id)
}

pub fn update_person(conn: &Connection, id: i64, input: &PersonInput) -> AppResult<Person> {
    let (name, external_id, note) = input.validated()?;
    let n = conn.execute(
        "UPDATE persons SET full_name = ?2, external_id = ?3, note = ?4,
            consent_at = COALESCE(?5, consent_at), updated_at = datetime('now')
         WHERE id = ?1",
        params![id, name, external_id, note, input.consent_at],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("shaxs"));
    }
    get_person(conn, id)
}

pub fn set_person_active(conn: &Connection, id: i64, active: bool) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE persons SET is_active = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![id, active],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("shaxs"));
    }
    Ok(())
}

/// O'chirish huquqi: embeddinglar CASCADE bilan o'chadi, jurnaldagi havola NULL bo'ladi.
pub fn delete_person(conn: &Connection, id: i64) -> AppResult<()> {
    if conn.execute("DELETE FROM persons WHERE id = ?1", [id])? == 0 {
        return Err(AppError::NotFound("shaxs"));
    }
    Ok(())
}

// ---------- Embeddinglar ----------

#[derive(Debug, Clone)]
pub struct NewEmbedding {
    pub model: String,
    pub vector: Vec<f32>,
    pub quality: Option<f32>,
    /// Tekislangan 112×112 yuz, PNG: model almashganda shundan qayta hisoblanadi (yo'qotishsiz format).
    pub thumb: Option<Vec<u8>>,
    pub device: Option<String>,
}

/// Matcher uchun: faqat faol shaxslarning berilgan modeldagi vektorlari.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEmbedding {
    pub id: i64,
    pub person_id: i64,
    pub vector: Vec<f32>,
}

fn insert_embeddings(conn: &Connection, person_id: i64, samples: &[NewEmbedding]) -> AppResult<()> {
    let mut st = conn.prepare_cached(
        "INSERT INTO face_embeddings (person_id, model, dim, vector, quality, thumb, device)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for s in samples {
        if s.vector.is_empty() {
            return Err(AppError::Invalid("bo'sh embedding".into()));
        }
        st.execute(params![
            person_id,
            s.model,
            s.vector.len() as i64,
            vec_to_blob(&s.vector),
            s.quality,
            s.thumb,
            s.device
        ])?;
    }
    Ok(())
}

pub fn add_embeddings(conn: &mut Connection, person_id: i64, samples: &[NewEmbedding]) -> AppResult<()> {
    let tx = conn.transaction()?;
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM persons WHERE id = ?1)", [person_id], |r| r.get(0))?;
    if !exists {
        return Err(AppError::NotFound("shaxs"));
    }
    insert_embeddings(&tx, person_id, samples)?;
    tx.execute("UPDATE persons SET updated_at = datetime('now') WHERE id = ?1", [person_id])?;
    tx.commit()?;
    Ok(())
}

pub fn load_embeddings(conn: &Connection, model: &str) -> AppResult<Vec<StoredEmbedding>> {
    let mut st = conn.prepare_cached(
        "SELECT e.id, e.person_id, e.vector FROM face_embeddings e
         JOIN persons p ON p.id = e.person_id
         WHERE e.model = ?1 AND p.is_active = 1
         ORDER BY e.id",
    )?;
    let rows = st.query_map([model], |r| {
        Ok(StoredEmbedding { id: r.get(0)?, person_id: r.get(1)?, vector: blob_to_vec(&r.get::<_, Vec<u8>>(2)?) })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn delete_embedding(conn: &Connection, id: i64) -> AppResult<()> {
    if conn.execute("DELETE FROM face_embeddings WHERE id = ?1", [id])? == 0 {
        return Err(AppError::NotFound("namuna"));
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReindexReport {
    pub updated: usize,
    /// Rasmi (thumb) saqlanmagan eski namunalar: ularni qayta hisoblab bo'lmaydi.
    pub skipped: usize,
}

/// Model almashtirilganda: boshqa modeldagi namunalarni saqlangan tekislangan rasmdan
/// (`thumb`) qayta hisoblaydi. Hammasi bitta tranzaksiyada — yarim yo'lda to'xtamaydi.
pub fn reembed_outdated(
    conn: &mut Connection,
    model: &str,
    mut embed: impl FnMut(&[u8]) -> AppResult<Vec<f32>>,
) -> AppResult<ReindexReport> {
    let tx = conn.transaction()?;
    let rows: Vec<(i64, Option<Vec<u8>>)> = {
        let mut st = tx.prepare("SELECT id, thumb FROM face_embeddings WHERE model <> ?1")?;
        let rows = st.query_map([model], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut report = ReindexReport::default();
    for (id, thumb) in rows {
        let Some(thumb) = thumb else {
            report.skipped += 1;
            continue;
        };
        let v = embed(&thumb)?;
        tx.execute(
            "UPDATE face_embeddings SET model = ?2, dim = ?3, vector = ?4 WHERE id = ?1",
            params![id, model, v.len() as i64, vec_to_blob(&v)],
        )?;
        report.updated += 1;
    }
    tx.commit()?;
    Ok(report)
}

// ---------- Jurnal ----------

#[derive(Debug, Clone)]
pub struct NewLog {
    pub person_id: Option<i64>,
    pub score: f32,
    pub matched: bool,
    pub source: String,
    pub snapshot: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: i64,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    pub score: f32,
    pub matched: bool,
    pub source: String,
    pub has_snapshot: bool,
    pub created_at: String,
}

pub fn insert_log(conn: &Connection, log: &NewLog) -> AppResult<i64> {
    conn.prepare_cached(
        "INSERT INTO recognition_logs (person_id, score, matched, source, snapshot) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?
    .execute(params![log.person_id, log.score, log.matched, log.source, log.snapshot])?;
    Ok(conn.last_insert_rowid())
}

/// Sahifalab o'qish: `before_id` dan eskiroq `limit` ta yozuv, yangilari birinchi.
pub fn list_logs(conn: &Connection, before_id: Option<i64>, limit: u32) -> AppResult<Vec<LogEntry>> {
    let mut st = conn.prepare_cached(
        "SELECT l.id, l.person_id, p.full_name, l.score, l.matched, l.source, l.snapshot IS NOT NULL, l.created_at
         FROM recognition_logs l LEFT JOIN persons p ON p.id = l.person_id
         WHERE (?1 IS NULL OR l.id < ?1)
         ORDER BY l.id DESC LIMIT ?2",
    )?;
    let rows = st.query_map(params![before_id, limit.min(500)], |r| {
        Ok(LogEntry {
            id: r.get(0)?,
            person_id: r.get(1)?,
            person_name: r.get(2)?,
            score: r.get(3)?,
            matched: r.get(4)?,
            source: r.get(5)?,
            has_snapshot: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Saqlash muddati siyosati: `days` kundan eski yozuvlarni o'chiradi.
pub fn purge_logs_older_than(conn: &Connection, days: u32) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM recognition_logs WHERE created_at < datetime('now', ?1)",
        [format!("-{days} days")],
    )?)
}

// ---------- Sozlamalar ----------

pub fn get_settings(conn: &Connection) -> AppResult<Vec<(String, String)>> {
    let mut st = conn.prepare_cached("SELECT key, value FROM settings ORDER BY key")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn sample(v: &[f32]) -> NewEmbedding {
        NewEmbedding { model: "sface".into(), vector: v.to_vec(), quality: Some(0.9), thumb: None, device: None }
    }

    fn input(name: &str) -> PersonInput {
        PersonInput { full_name: name.into(), ..Default::default() }
    }

    #[test]
    fn create_with_samples_and_read_back() {
        let mut c = open_in_memory().unwrap();
        let p = create_person(&mut c, &input("  Ali Valiyev "), &[sample(&[1.0, 0.0]), sample(&[0.0, 1.0])]).unwrap();
        assert_eq!(p.full_name, "Ali Valiyev");
        assert_eq!(p.sample_count, 2);
        assert!(p.is_active);
        assert_eq!(p.uuid.len(), 36);
        let e = load_embeddings(&c, "sface").unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[1].vector, vec![0.0, 1.0]);
        assert!(load_embeddings(&c, "other-model").unwrap().is_empty());
    }

    #[test]
    fn failed_sample_rolls_back_person() {
        let mut c = open_in_memory().unwrap();
        let err = create_person(&mut c, &input("Ali"), &[sample(&[1.0]), sample(&[])]);
        assert!(matches!(err, Err(AppError::Invalid(_))));
        assert!(list_persons(&c, None, true).unwrap().is_empty());
        assert!(load_embeddings(&c, "sface").unwrap().is_empty());
    }

    #[test]
    fn empty_name_rejected() {
        let mut c = open_in_memory().unwrap();
        assert!(matches!(create_person(&mut c, &input("   "), &[]), Err(AppError::Invalid(_))));
    }

    #[test]
    fn search_and_inactive_filter() {
        let mut c = open_in_memory().unwrap();
        let a = create_person(&mut c, &input("Ali"), &[sample(&[1.0])]).unwrap();
        create_person(&mut c, &PersonInput { external_id: Some("EMP_50%".into()), ..input("bobur") }, &[]).unwrap();
        create_person(&mut c, &input("Aziz"), &[]).unwrap();

        let names = |v: Vec<Person>| v.into_iter().map(|p| p.full_name).collect::<Vec<_>>();
        assert_eq!(names(list_persons(&c, None, false).unwrap()), ["Ali", "Aziz", "bobur"]);
        assert_eq!(names(list_persons(&c, Some("li"), false).unwrap()), ["Ali"]);
        // LIKE maxsus belgilari so'zma-so'z qidiriladi
        assert_eq!(names(list_persons(&c, Some("50%"), false).unwrap()), ["bobur"]);
        assert!(list_persons(&c, Some("_"), false).unwrap().iter().all(|p| p.full_name == "bobur"));

        set_person_active(&c, a.id, false).unwrap();
        assert_eq!(names(list_persons(&c, None, false).unwrap()), ["Aziz", "bobur"]);
        assert_eq!(list_persons(&c, None, true).unwrap().len(), 3);
        assert!(load_embeddings(&c, "sface").unwrap().is_empty(), "nofaol shaxs Matcher'ga tushmaydi");
    }

    #[test]
    fn update_keeps_consent_when_not_given() {
        let mut c = open_in_memory().unwrap();
        let p = create_person(&mut c, &PersonInput { consent_at: Some("2026-10-05".into()), ..input("Ali") }, &[]).unwrap();
        let u = update_person(&c, p.id, &PersonInput { note: Some(" izoh ".into()), ..input("Ali V") }).unwrap();
        assert_eq!(u.full_name, "Ali V");
        assert_eq!(u.note.as_deref(), Some("izoh"));
        assert_eq!(u.consent_at.as_deref(), Some("2026-10-05"));
        assert!(matches!(update_person(&c, 999, &input("X")), Err(AppError::NotFound(_))));
    }

    #[test]
    fn delete_cascades_embeddings_and_nulls_logs() {
        let mut c = open_in_memory().unwrap();
        let p = create_person(&mut c, &input("Ali"), &[sample(&[1.0])]).unwrap();
        insert_log(&c, &NewLog { person_id: Some(p.id), score: 0.8, matched: true, source: "live".into(), snapshot: None }).unwrap();
        delete_person(&c, p.id).unwrap();
        assert!(load_embeddings(&c, "sface").unwrap().is_empty());
        let n: i64 = c.query_row("SELECT COUNT(*) FROM face_embeddings", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let logs = list_logs(&c, None, 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].person_id, None);
        assert!(matches!(delete_person(&c, p.id), Err(AppError::NotFound(_))));
    }

    #[test]
    fn add_and_delete_samples() {
        let mut c = open_in_memory().unwrap();
        let p = create_person(&mut c, &input("Ali"), &[]).unwrap();
        add_embeddings(&mut c, p.id, &[sample(&[0.5, 0.5])]).unwrap();
        assert_eq!(get_person(&c, p.id).unwrap().sample_count, 1);
        assert!(matches!(add_embeddings(&mut c, 999, &[sample(&[1.0])]), Err(AppError::NotFound(_))));
        let id = load_embeddings(&c, "sface").unwrap()[0].id;
        delete_embedding(&c, id).unwrap();
        assert_eq!(get_person(&c, p.id).unwrap().sample_count, 0);
    }

    #[test]
    fn reembed_outdated_uses_thumbs() {
        let mut c = open_in_memory().unwrap();
        let old = |thumb: Option<Vec<u8>>| NewEmbedding { model: "old".into(), thumb, ..sample(&[1.0, 0.0]) };
        create_person(&mut c, &input("Ali"), &[old(Some(vec![7])), old(None), sample(&[0.0, 1.0])]).unwrap();

        let r = reembed_outdated(&mut c, "sface", |thumb| Ok(vec![thumb[0] as f32, 0.0, 0.0])).unwrap();
        assert_eq!(r, ReindexReport { updated: 1, skipped: 1 });
        let v: Vec<_> = load_embeddings(&c, "sface").unwrap().into_iter().map(|e| e.vector).collect();
        assert_eq!(v, [vec![7.0, 0.0, 0.0], vec![0.0, 1.0]]);
        // Takroriy chaqiruv hech narsani o'zgartirmaydi (rasmsizi qoladi)
        assert_eq!(reembed_outdated(&mut c, "sface", |_| unreachable!()).unwrap(), ReindexReport { updated: 0, skipped: 1 });
    }

    #[test]
    fn reembed_failure_rolls_back() {
        let mut c = open_in_memory().unwrap();
        let old = NewEmbedding { model: "old".into(), thumb: Some(vec![1]), ..sample(&[1.0]) };
        create_person(&mut c, &input("Ali"), &[old.clone(), old]).unwrap();
        let mut n = 0;
        let r = reembed_outdated(&mut c, "sface", |_| {
            n += 1;
            if n == 2 { Err(AppError::Invalid("x".into())) } else { Ok(vec![1.0]) }
        });
        assert!(r.is_err());
        assert!(load_embeddings(&c, "sface").unwrap().is_empty());
        assert_eq!(load_embeddings(&c, "old").unwrap().len(), 2);
    }

    #[test]
    fn logs_paginate_newest_first_and_purge() {
        let c = open_in_memory().unwrap();
        for i in 0..5 {
            insert_log(&c, &NewLog { person_id: None, score: i as f32, matched: false, source: "live".into(), snapshot: None }).unwrap();
        }
        let page1 = list_logs(&c, None, 2).unwrap();
        assert_eq!(page1.iter().map(|l| l.id).collect::<Vec<_>>(), [5, 4]);
        let page2 = list_logs(&c, Some(page1[1].id), 2).unwrap();
        assert_eq!(page2.iter().map(|l| l.id).collect::<Vec<_>>(), [3, 2]);

        c.execute("UPDATE recognition_logs SET created_at = datetime('now', '-40 days') WHERE id <= 2", []).unwrap();
        assert_eq!(purge_logs_older_than(&c, 30).unwrap(), 2);
        assert_eq!(list_logs(&c, None, 10).unwrap().len(), 3);
    }

    #[test]
    fn settings_upsert() {
        let c = open_in_memory().unwrap();
        assert_eq!(get_setting(&c, "threshold").unwrap(), None);
        set_setting(&c, "threshold", "0.36").unwrap();
        set_setting(&c, "threshold", "0.40").unwrap();
        set_setting(&c, "locale", "uz").unwrap();
        assert_eq!(get_setting(&c, "threshold").unwrap().as_deref(), Some("0.40"));
        assert_eq!(get_settings(&c).unwrap().len(), 2);
    }
}
