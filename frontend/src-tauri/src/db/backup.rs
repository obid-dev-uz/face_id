//! Zaxira nusxa va tiklash. SQLite Online Backup API ishlatiladi: ilova ishlab
//! turganda ham izchil nusxa olinadi (WAL'dagi yozuvlar bilan birga).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{backup::Progress, Connection, OpenFlags, MAIN_DB};

use crate::error::{AppError, AppResult};

/// `dest` ga to'liq nusxa yozadi. Avval vaqtinchalik faylga yoziladi, keyin
/// almashtiriladi — yarim yozilgan zaxira hech qachon `dest` nomida qolmaydi.
pub fn backup_to(conn: &Connection, dest: &Path) -> AppResult<()> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("part");
    let _ = fs::remove_file(&tmp);
    if let Err(e) = conn.backup(MAIN_DB, &tmp, None) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

/// Fayl KHOFace bazasi ekanini, buzilmaganini va bizdan yangi sxemada emasligini tekshiradi.
pub fn validate(src: &Path) -> AppResult<u32> {
    let invalid = |m: &str| AppError::Invalid(format!("zaxira fayli yaroqsiz: {m}"));
    let conn = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| invalid("ochib bo'lmadi"))?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|_| invalid("SQLite fayli emas"))?;
    if check != "ok" {
        return Err(invalid("fayl buzilgan"));
    }
    let version = super::schema_version(&conn)?;
    if version == 0 {
        return Err(invalid("KHOFace bazasi emas"));
    }
    if version > super::latest_version() {
        return Err(invalid("ilovaning yangiroq versiyasida yaratilgan"));
    }
    let tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('persons', 'face_embeddings')",
        [],
        |r| r.get(0),
    )?;
    if tables != 2 {
        return Err(invalid("KHOFace bazasi emas"));
    }
    Ok(version)
}

/// Joriy bazani `src` mazmuni bilan almashtiradi. Oldin joriy holat `safety_copy` ga
/// saqlanadi, keyin eski zaxiralar uchun migratsiyalar qo'llanadi.
pub fn restore_from(conn: &mut Connection, src: &Path, safety_copy: &Path) -> AppResult<()> {
    validate(src)?;
    backup_to(conn, safety_copy)?;
    conn.restore(MAIN_DB, src, None::<fn(Progress)>)?;
    super::migrate(conn)?;
    Ok(())
}

/// `<data_dir>/backups/<prefix>-<unix soniya>.db`
pub fn default_path(data_dir: &Path, prefix: &str) -> PathBuf {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    data_dir.join("backups").join(format!("{prefix}-{ts}.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, repo};

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("khoface-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn person(c: &mut Connection, name: &str) {
        let input = repo::PersonInput { full_name: name.into(), ..Default::default() };
        repo::create_person(c, &input, &[]).unwrap();
    }

    fn names(c: &Connection) -> Vec<String> {
        repo::list_persons(c, None, true).unwrap().into_iter().map(|p| p.full_name).collect()
    }

    #[test]
    fn backup_then_restore_roundtrip() {
        let d = tmpdir("roundtrip");
        let mut c = db::open(&d.join("live.db")).unwrap();
        person(&mut c, "Ali");
        backup_to(&c, &d.join("b.db")).unwrap();
        assert!(!d.join("b.part").exists());

        person(&mut c, "Vali");
        assert_eq!(names(&c), ["Ali", "Vali"]);

        restore_from(&mut c, &d.join("b.db"), &d.join("safety.db")).unwrap();
        assert_eq!(names(&c), ["Ali"]);
        // Tiklashdan oldingi holat saqlangan
        let safety = db::open(&d.join("safety.db")).unwrap();
        assert_eq!(names(&safety), ["Ali", "Vali"]);
        // Tiklangandan keyin ham yozish ishlaydi va FK yoqilgan
        person(&mut c, "Soli");
        let fk: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rejects_foreign_or_broken_files() {
        let d = tmpdir("reject");
        let mut c = db::open(&d.join("live.db")).unwrap();
        person(&mut c, "Ali");

        fs::write(d.join("text.db"), b"bu baza emas").unwrap();
        let other = Connection::open(d.join("other.db")).unwrap();
        other.execute_batch("CREATE TABLE t(x); PRAGMA user_version = 1;").unwrap();
        drop(other);
        let newer = db::open(&d.join("newer.db")).unwrap();
        newer.pragma_update(None, "user_version", db::latest_version() + 1).unwrap();
        drop(newer);

        for f in ["text.db", "other.db", "newer.db", "missing.db"] {
            let r = restore_from(&mut c, &d.join(f), &d.join("safety.db"));
            assert!(matches!(r, Err(AppError::Invalid(_))), "{f}: {r:?}");
        }
        assert_eq!(names(&c), ["Ali"], "rad etilgan tiklash bazaga tegmaydi");
        assert!(!d.join("safety.db").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
