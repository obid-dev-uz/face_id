use std::path::Path;

use rusqlite::Connection;

pub mod backup;
pub mod repo;

const MIGRATIONS: &[&str] = &[include_str!("migrations/001_init.sql"), include_str!("migrations/002_admin.sql")];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(mut conn: Connection) -> rusqlite::Result<Connection> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // O'chirilgan yozuvlar (yuz rasmlari, vektorlar) faylda nol bilan qayta yoziladi.
    conn.pragma_update(None, "secure_delete", "ON")?;
    migrate(&mut conn)?;
    Ok(conn)
}

/// Yetishmayotgan migratsiyalarni qo'llaydi (zaxiradan tiklangandan keyin ham chaqiriladi).
pub fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let cur = schema_version(conn)?;
    for (i, sql) in (1u32..).zip(MIGRATIONS).skip(cur as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn latest_version() -> u32 {
    MIGRATIONS.len() as u32
}

pub fn schema_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
}

pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(v).to_vec()
}

pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    bytemuck::pod_collect_to_vec(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once() {
        let conn = init(Connection::open_in_memory().unwrap()).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.len() as u32);
        let conn = init(conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.len() as u32);
    }

    #[test]
    fn vector_blob_roundtrip() {
        let v = [0.25f32, -1.0, 3.5];
        assert_eq!(blob_to_vec(&vec_to_blob(&v)), v);
    }
}
