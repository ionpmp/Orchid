//! Firefox history cleanup. `places.sqlite` also stores bookmarks, so the
//! history tables are cleared and only unbookmarked places are deleted.

use std::path::Path;

use rusqlite::Connection;

/// Remove visits from a Firefox `places.sqlite`. Bookmarked places stay.
///
/// A missing file is success and removes nothing. A locked file returns an error
/// so the widget can ask the user to close Firefox.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or is not a places database.
pub fn clear_history(places_sqlite: &Path) -> Result<u64, String> {
    if !places_sqlite.is_file() {
        return Ok(0);
    }
    let conn = Connection::open(places_sqlite).map_err(|e| format!("open places: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_millis(400))
        .map_err(|e| format!("places busy timeout: {e}"))?;
    if !has_table(&conn, "moz_places") || !has_table(&conn, "moz_bookmarks") {
        return Err("not a Firefox places database".into());
    }
    let removed = conn
        .query_row("SELECT COUNT(*) FROM moz_historyvisits", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0)
        .max(0) as u64;
    if has_table(&conn, "moz_historyvisits") {
        conn.execute("DELETE FROM moz_historyvisits", [])
            .map_err(|e| format!("clear visits: {e}"))?;
    }
    if has_table(&conn, "moz_inputhistory") {
        conn.execute("DELETE FROM moz_inputhistory", [])
            .map_err(|e| format!("clear input history: {e}"))?;
    }
    conn.execute(
        "UPDATE moz_places SET visit_count = 0, last_visit_date = NULL",
        [],
    )
    .map_err(|e| format!("reset visit counts: {e}"))?;
    let keyword_clause = if has_table(&conn, "moz_keywords") {
        "AND id NOT IN (SELECT IFNULL(place_id, 0) FROM moz_keywords)"
    } else {
        ""
    };
    let sql = format!(
        "DELETE FROM moz_places \
         WHERE visit_count = 0 \
         AND id NOT IN (SELECT IFNULL(fk, 0) FROM moz_bookmarks) \
         {keyword_clause}"
    );
    conn.execute(&sql, [])
        .map_err(|e| format!("delete unbookmarked places: {e}"))?;
    let _ = conn.execute("VACUUM", []);
    Ok(removed)
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_rows_go_and_bookmarks_stay() {
        let root = tempfile::tempdir().expect("temp");
        let path = root.path().join("places.sqlite");
        let conn = Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE moz_places (
                id INTEGER PRIMARY KEY,
                url TEXT,
                visit_count INTEGER,
                last_visit_date INTEGER,
                foreign_count INTEGER DEFAULT 0
            );
            CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, fk INTEGER, title TEXT);
            CREATE TABLE moz_historyvisits (id INTEGER PRIMARY KEY, place_id INTEGER);
            CREATE TABLE moz_inputhistory (place_id INTEGER, input TEXT);
            CREATE TABLE moz_keywords (id INTEGER PRIMARY KEY, place_id INTEGER);
            INSERT INTO moz_places (id, url, visit_count, last_visit_date) VALUES
                (1, 'https://kept.example/', 4, 10),
                (2, 'https://gone.example/', 2, 11);
            INSERT INTO moz_bookmarks (id, fk, title) VALUES (1, 1, 'Kept');
            INSERT INTO moz_historyvisits (id, place_id) VALUES (1, 1), (2, 2);
            INSERT INTO moz_inputhistory (place_id, input) VALUES (2, 'gone');",
        )
        .expect("schema");
        drop(conn);

        let removed = clear_history(&path).expect("clear");
        assert_eq!(removed, 2);
        let conn = Connection::open(&path).expect("reopen");
        let urls: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT url FROM moz_places ORDER BY id")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(urls, vec!["https://kept.example/".to_string()]);
        let visits: i64 = conn
            .query_row("SELECT COUNT(*) FROM moz_historyvisits", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(visits, 0);
        assert_eq!(
            clear_history(root.path().join("missing.sqlite").as_path()).unwrap(),
            0
        );
    }
}
