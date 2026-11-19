use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Tag filter for statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagFilter<'a> {
    All,
    Untagged,
    Tag(&'a str),
}

impl<'a> TagFilter<'a> {
    pub fn from_opt(tag: Option<&'a str>) -> Self {
        match tag {
            None => Self::All,
            Some("") | Some("__untagged__") => Self::Untagged,
            Some(s) => Self::Tag(s),
        }
    }
}

// ---------------------------------------------------------------------------
// Profiles CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimerProfile {
    pub id: i64,
    pub name: String,
    pub time_work_secs: u32,
    pub time_short_break_secs: u32,
    pub time_long_break_secs: u32,
    pub long_break_interval: u32,
    pub short_breaks_enabled: bool,
    pub long_breaks_enabled: bool,
    pub is_builtin: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileInput {
    pub name: String,
    pub time_work_secs: u32,
    pub time_short_break_secs: u32,
    pub time_long_break_secs: u32,
    pub long_break_interval: u32,
    pub short_breaks_enabled: bool,
    pub long_breaks_enabled: bool,
}

fn map_profile_row(r: &rusqlite::Row<'_>) -> Result<TimerProfile> {
    Ok(TimerProfile {
        id: r.get(0)?,
        name: r.get(1)?,
        time_work_secs: r.get(2)?,
        time_short_break_secs: r.get(3)?,
        time_long_break_secs: r.get(4)?,
        long_break_interval: r.get(5)?,
        short_breaks_enabled: r.get::<_, i64>(6)? != 0,
        long_breaks_enabled: r.get::<_, i64>(7)? != 0,
        is_builtin: r.get::<_, i64>(8)? != 0,
        sort_order: r.get(9)?,
    })
}

pub fn list_profiles(conn: &Connection) -> Result<Vec<TimerProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, time_work_secs, time_short_break_secs, time_long_break_secs,
                long_break_interval, short_breaks_enabled, long_breaks_enabled,
                is_builtin, sort_order
         FROM profiles
         ORDER BY sort_order ASC, id ASC",
    )?;
    let rows = stmt.query_map([], map_profile_row)?.collect();
    rows
}

pub fn get_profile(conn: &Connection, id: i64) -> Result<Option<TimerProfile>> {
    conn.query_row(
        "SELECT id, name, time_work_secs, time_short_break_secs, time_long_break_secs,
                long_break_interval, short_breaks_enabled, long_breaks_enabled,
                is_builtin, sort_order
         FROM profiles WHERE id = ?1",
        [id],
        map_profile_row,
    )
    .optional()
}

pub fn create_profile(conn: &Connection, input: &ProfileInput) -> Result<TimerProfile> {
    let name = input.name.trim();
    let max_order: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM profiles",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO profiles
            (name, time_work_secs, time_short_break_secs, time_long_break_secs,
             long_break_interval, short_breaks_enabled, long_breaks_enabled, is_builtin, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
        params![
            name,
            input.time_work_secs.clamp(60, 5400),
            input.time_short_break_secs.clamp(60, 5400),
            input.time_long_break_secs.clamp(60, 5400),
            input.long_break_interval.clamp(1, 12),
            input.short_breaks_enabled as i64,
            input.long_breaks_enabled as i64,
            max_order + 1,
        ],
    )?;
    let id = conn.last_insert_rowid();
    Ok(get_profile(conn, id)?.expect("profile just inserted"))
}

pub fn update_profile(conn: &Connection, id: i64, input: &ProfileInput) -> Result<TimerProfile> {
    let name = input.name.trim();
    conn.execute(
        "UPDATE profiles
         SET name = ?1,
             time_work_secs = ?2,
             time_short_break_secs = ?3,
             time_long_break_secs = ?4,
             long_break_interval = ?5,
             short_breaks_enabled = ?6,
             long_breaks_enabled = ?7
         WHERE id = ?8",
        params![
            name,
            input.time_work_secs.clamp(60, 5400),
            input.time_short_break_secs.clamp(60, 5400),
            input.time_long_break_secs.clamp(60, 5400),
            input.long_break_interval.clamp(1, 12),
            input.short_breaks_enabled as i64,
            input.long_breaks_enabled as i64,
            id,
        ],
    )?;
    get_profile(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub fn delete_profile(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM profiles WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tags CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color FROM tags ORDER BY name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?
        .collect();
    rows
}

pub fn upsert_tag(conn: &Connection, name: &str, color: Option<&str>) -> Result<Tag> {
    let trimmed = name.trim();
    conn.execute(
        "INSERT INTO tags (name, color, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET color = COALESCE(excluded.color, tags.color)",
        params![trimmed, color, unix_now()],
    )?;
    conn.query_row(
        "SELECT id, name, color FROM tags WHERE name = ?1 COLLATE NOCASE",
        [trimmed],
        |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        },
    )
}

pub fn delete_tag(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
    Ok(())
}

/// Returns `(distinct_non_empty_tags, has_untagged_work_sessions)` across saved tags and recorded sessions.
pub fn list_used_session_tags(conn: &Connection) -> Result<(Vec<String>, bool)> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT TRIM(tag) AS t
         FROM sessions
         WHERE round_type = 'work' AND completed = 1 AND tag IS NOT NULL AND TRIM(tag) != ''
         UNION
         SELECT name AS t FROM tags
         ORDER BY t COLLATE NOCASE ASC",
    )?;
    let tags: Vec<String> = stmt.query_map([], |r| r.get(0))?.flatten().collect();

    let has_untagged: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sessions
            WHERE round_type = 'work' AND completed = 1
              AND (tag IS NULL OR TRIM(tag) = '')
        )",
        [],
        |r| r.get(0),
    )?;
    Ok((tags, has_untagged))
}

// ---------------------------------------------------------------------------
// Session CRUD (DATA-03)
// ---------------------------------------------------------------------------

/// Inserts a new session row when a round begins.
/// Returns the row ID so it can be passed to `complete_session` later.
pub fn insert_session(
    conn: &Connection,
    round_type: &str,
    duration_secs: u32,
    tag: Option<&str>,
) -> Result<i64> {
    let started_at = unix_now();
    let clean_tag = tag.map(str::trim).filter(|s| !s.is_empty());
    conn.execute(
        "INSERT INTO sessions (started_at, round_type, duration_secs, completed, tag)
         VALUES (?1, ?2, ?3, 0, ?4)",
        params![started_at, round_type, duration_secs, clean_tag],
    )?;
    let id = conn.last_insert_rowid();
    log::debug!(
        "[db] session started: id={id} type={round_type} duration={duration_secs}s tag={:?}",
        clean_tag
    );
    Ok(id)
}

/// Updates the tag of an in-progress session when the user changes it mid-round.
pub fn update_session_tag(conn: &Connection, session_id: i64, tag: Option<&str>) -> Result<()> {
    let clean_tag = tag.map(str::trim).filter(|s| !s.is_empty());
    conn.execute(
        "UPDATE sessions SET tag = ?1 WHERE id = ?2",
        params![clean_tag, session_id],
    )?;
    log::debug!("[db] session tag updated: id={session_id} tag={:?}", clean_tag);
    Ok(())
}

/// Updates the duration of an in-progress session when adjusted via +5/-5 or live settings.
pub fn update_session_duration(
    conn: &Connection,
    session_id: i64,
    duration_secs: u32,
) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET duration_secs = ?1 WHERE id = ?2",
        params![duration_secs, session_id],
    )?;
    log::debug!("[db] session duration updated: id={session_id} duration={duration_secs}s");
    Ok(())
}

/// Updates a session when the round ends (by completion or skip).
pub fn complete_session(conn: &Connection, session_id: i64, completed: bool) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET ended_at = ?1, completed = ?2 WHERE id = ?3",
        params![unix_now(), completed as i64, session_id],
    )?;
    log::debug!("[db] session ended: id={session_id} completed={completed}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Stats queries
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub total_work_sessions: i64,
    pub completed_work_sessions: i64,
    /// Sum of duration_secs for all *completed* work sessions.
    pub total_work_secs: i64,
}

#[derive(Debug, Serialize)]
pub struct TagStat {
    /// None represents untagged sessions.
    pub tag: Option<String>,
    pub rounds: u32,
    pub focus_mins: u32,
}

fn tag_clause_and_param<'a>(filter: TagFilter<'a>) -> (&'static str, Option<&'a str>) {
    match filter {
        TagFilter::All => ("", None),
        TagFilter::Untagged => (" AND (tag IS NULL OR TRIM(tag) = '')", None),
        TagFilter::Tag(t) => (" AND LOWER(TRIM(tag)) = LOWER(TRIM(?1))", Some(t)),
    }
}

pub fn get_all_time_stats(conn: &Connection, filter: TagFilter<'_>) -> Result<SessionStats> {
    let (clause, param) = tag_clause_and_param(filter);

    let total_sql = format!("SELECT COUNT(*) FROM sessions WHERE round_type = 'work'{clause}");
    let completed_sql = format!(
        "SELECT COUNT(*) FROM sessions WHERE round_type = 'work' AND completed = 1{clause}"
    );
    let secs_sql = format!(
        "SELECT COALESCE(SUM(duration_secs), 0)
         FROM sessions WHERE round_type = 'work' AND completed = 1{clause}"
    );

    let (total_work_sessions, completed_work_sessions, total_work_secs) = match param {
        Some(t) => (
            conn.query_row(&total_sql, [t], |r| r.get(0))?,
            conn.query_row(&completed_sql, [t], |r| r.get(0))?,
            conn.query_row(&secs_sql, [t], |r| r.get(0))?,
        ),
        None => (
            conn.query_row(&total_sql, [], |r| r.get(0))?,
            conn.query_row(&completed_sql, [], |r| r.get(0))?,
            conn.query_row(&secs_sql, [], |r| r.get(0))?,
        ),
    };

    Ok(SessionStats {
        total_work_sessions,
        completed_work_sessions,
        total_work_secs,
    })
}

// ---------------------------------------------------------------------------
// Detailed stats queries (DATA-04)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DailyStats {
    pub rounds: u32,
    pub focus_mins: u32,
    /// None when no work sessions were started today (avoids 0/0).
    pub completion_rate: Option<f32>,
    /// Completed work rounds per hour of the day (index 0 = midnight).
    pub by_hour: Vec<u32>,
    /// Focus breakdown by tag for today (unfiltered so user sees distribution).
    pub by_tag: Vec<TagStat>,
}

#[derive(Debug, Serialize)]
pub struct DayStat {
    /// Local calendar date in "YYYY-MM-DD" format.
    pub date: String,
    pub rounds: u32,
}

#[derive(Debug, Serialize)]
pub struct HeatmapEntry {
    /// Local calendar date in "YYYY-MM-DD" format.
    pub date: String,
    pub count: u32,
}

#[derive(Debug, Serialize)]
pub struct StreakInfo {
    pub current: u32,
    pub longest: u32,
}

/// Focus breakdown grouped by tag for a given SQL date predicate.
fn query_tag_breakdown(conn: &Connection, date_predicate: &str) -> Result<Vec<TagStat>> {
    let sql = format!(
        "SELECT CASE WHEN tag IS NULL OR TRIM(tag) = '' THEN NULL ELSE TRIM(tag) END AS t,
                COUNT(*) AS rounds,
                COALESCE(SUM(duration_secs), 0) AS total_secs
         FROM sessions
         WHERE round_type = 'work' AND completed = 1 {date_predicate}
         GROUP BY LOWER(COALESCE(TRIM(tag), ''))
         ORDER BY total_secs DESC, rounds DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            let total_secs: i64 = r.get(2)?;
            Ok(TagStat {
                tag: r.get(0)?,
                rounds: r.get(1)?,
                focus_mins: ((total_secs + 30) / 60) as u32,
            })
        })?
        .collect();
    rows
}

pub fn get_weekly_tag_breakdown(conn: &Connection) -> Result<Vec<TagStat>> {
    query_tag_breakdown(
        conn,
        "AND date(started_at, 'unixepoch', 'localtime') >= date('now', 'localtime', '-6 days')",
    )
}

/// Completed work rounds and focus time for today (local calendar date).
pub fn get_daily_stats(conn: &Connection, filter: TagFilter<'_>) -> Result<DailyStats> {
    let today: String = conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?;

    let (tag_sql, tag_param) = match filter {
        TagFilter::All => ("", None),
        TagFilter::Untagged => (" AND (tag IS NULL OR TRIM(tag) = '')", None),
        TagFilter::Tag(t) => (" AND LOWER(TRIM(tag)) = LOWER(TRIM(?2))", Some(t)),
    };

    let total_q = format!(
        "SELECT COUNT(*) FROM sessions
         WHERE round_type = 'work'
           AND date(started_at, 'unixepoch', 'localtime') = ?1{tag_sql}"
    );
    let completed_q = format!(
        "SELECT COUNT(*) FROM sessions
         WHERE round_type = 'work' AND completed = 1
           AND date(started_at, 'unixepoch', 'localtime') = ?1{tag_sql}"
    );
    let focus_q = format!(
        "SELECT COALESCE(SUM(duration_secs), 0) FROM sessions
         WHERE round_type = 'work' AND completed = 1
           AND date(started_at, 'unixepoch', 'localtime') = ?1{tag_sql}"
    );
    let hour_q = format!(
        "SELECT CAST(strftime('%H', datetime(started_at, 'unixepoch', 'localtime')) AS INTEGER) as h,
                COUNT(*) as cnt
         FROM sessions
         WHERE round_type = 'work' AND completed = 1
           AND date(started_at, 'unixepoch', 'localtime') = ?1{tag_sql}
         GROUP BY h"
    );

    let (total, completed, focus_secs): (i64, i64, i64) = match tag_param {
        Some(t) => (
            conn.query_row(&total_q, params![&today, t], |r| r.get(0))?,
            conn.query_row(&completed_q, params![&today, t], |r| r.get(0))?,
            conn.query_row(&focus_q, params![&today, t], |r| r.get(0))?,
        ),
        None => (
            conn.query_row(&total_q, [&today], |r| r.get(0))?,
            conn.query_row(&completed_q, [&today], |r| r.get(0))?,
            conn.query_row(&focus_q, [&today], |r| r.get(0))?,
        ),
    };

    let mut by_hour = vec![0u32; 24];
    let mut stmt = conn.prepare(&hour_q)?;
    let rows: Vec<(i64, u32)> = match tag_param {
        Some(t) => stmt
            .query_map(params![&today, t], |r| Ok((r.get(0)?, r.get(1)?)))?
            .flatten()
            .collect(),
        None => stmt
            .query_map([&today], |r| Ok((r.get(0)?, r.get(1)?)))?
            .flatten()
            .collect(),
    };
    for (h, cnt) in rows {
        if (0..24).contains(&h) {
            by_hour[h as usize] = cnt;
        }
    }

    let by_tag = query_tag_breakdown(
        conn,
        "AND date(started_at, 'unixepoch', 'localtime') = date('now', 'localtime')",
    )?;

    Ok(DailyStats {
        rounds: completed as u32,
        focus_mins: ((focus_secs + 30) / 60) as u32,
        completion_rate: if total > 0 {
            Some(completed as f32 / total as f32)
        } else {
            None
        },
        by_hour,
        by_tag,
    })
}

/// Completed work rounds per local calendar day for the last 7 days.
pub fn get_weekly_stats(conn: &Connection, filter: TagFilter<'_>) -> Result<Vec<DayStat>> {
    let (clause, param) = tag_clause_and_param(filter);
    let sql = format!(
        "SELECT date(started_at, 'unixepoch', 'localtime') as day,
                COUNT(*) as rounds
         FROM sessions
         WHERE round_type = 'work' AND completed = 1
           AND date(started_at, 'unixepoch', 'localtime') >= date('now', 'localtime', '-6 days'){clause}
         GROUP BY day
         ORDER BY day"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = match param {
        Some(t) => stmt
            .query_map([t], |r| Ok(DayStat { date: r.get(0)?, rounds: r.get(1)? }))?
            .collect(),
        None => stmt
            .query_map([], |r| Ok(DayStat { date: r.get(0)?, rounds: r.get(1)? }))?
            .collect(),
    };
    rows
}

/// Completed work rounds per local calendar day, all time (no date limit).
/// The frontend slices this into per-year views for navigation.
pub fn get_heatmap_data(conn: &Connection, filter: TagFilter<'_>) -> Result<Vec<HeatmapEntry>> {
    let (clause, param) = tag_clause_and_param(filter);
    let sql = format!(
        "SELECT date(started_at, 'unixepoch', 'localtime') as day,
                COUNT(*) as cnt
         FROM sessions
         WHERE round_type = 'work' AND completed = 1{clause}
         GROUP BY day
         ORDER BY day"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = match param {
        Some(t) => stmt
            .query_map([t], |r| Ok(HeatmapEntry { date: r.get(0)?, count: r.get(1)? }))?
            .collect(),
        None => stmt
            .query_map([], |r| Ok(HeatmapEntry { date: r.get(0)?, count: r.get(1)? }))?
            .collect(),
    };
    rows
}

/// Current and longest work-session streaks (consecutive local calendar days).
/// A streak stays active until midnight: if yesterday had sessions but today does not,
/// the streak is still counted as current.
pub fn get_streak(conn: &Connection, filter: TagFilter<'_>) -> Result<StreakInfo> {
    let today: String = conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?;
    let (clause, param) = tag_clause_and_param(filter);
    let sql = format!(
        "SELECT date(started_at, 'unixepoch', 'localtime') as day
         FROM sessions
         WHERE round_type = 'work' AND completed = 1{clause}
         GROUP BY day
         ORDER BY day"
    );
    let mut stmt = conn.prepare(&sql)?;
    let days: Vec<String> = match param {
        Some(t) => stmt.query_map([t], |r| r.get(0))?.flatten().collect(),
        None => stmt.query_map([], |r| r.get(0))?.flatten().collect(),
    };

    Ok(compute_streak(&days, &today))
}

// ---------------------------------------------------------------------------
// Streak helpers
// ---------------------------------------------------------------------------

/// Convert a "YYYY-MM-DD" string to a day number for arithmetic comparison.
/// Uses the proleptic Gregorian calendar; absolute value is arbitrary — only
/// differences between dates matter.
fn date_to_day_num(s: &str) -> Option<i32> {
    let mut parts = s.splitn(3, '-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: i32 = parts.next()?.parse().ok()?;
    let d: i32 = parts.next()?.parse().ok()?;
    let y = if m <= 2 { y - 1 } else { y };
    let m = if m <= 2 { m + 12 } else { m };
    Some(y * 365 + y / 4 - y / 100 + y / 400 + (153 * m - 457) / 5 + d)
}

pub fn compute_streak(days: &[String], today: &str) -> StreakInfo {
    let nums: Vec<i32> = days.iter().filter_map(|s| date_to_day_num(s)).collect();
    if nums.is_empty() {
        return StreakInfo { current: 0, longest: 0 };
    }

    let today_n = match date_to_day_num(today) {
        Some(n) => n,
        None => return StreakInfo { current: 0, longest: 0 },
    };

    // Current streak — alive if most recent session day is today or yesterday.
    let last = *nums.last().unwrap();
    let current = if last == today_n || last == today_n - 1 {
        let mut count = 0u32;
        let mut expected = last;
        for &n in nums.iter().rev() {
            if n == expected {
                count += 1;
                expected -= 1;
            } else {
                break;
            }
        }
        count
    } else {
        0
    };

    // Longest streak.
    let mut longest = 1u32;
    let mut run = 1u32;
    for i in 1..nums.len() {
        if nums[i] == nums[i - 1] + 1 {
            run += 1;
            if run > longest {
                longest = run;
            }
        } else {
            run = 1;
        }
    }

    StreakInfo { current, longest }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn insert_and_complete_session() {
        let conn = setup();
        let id = insert_session(&conn, "work", 1500, Some("Study")).unwrap();
        assert!(id > 0);

        complete_session(&conn, id, true).unwrap();

        let (completed, tag): (i64, Option<String>) = conn
            .query_row(
                "SELECT completed, tag FROM sessions WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(completed, 1);
        assert_eq!(tag.as_deref(), Some("Study"));
    }

    #[test]
    fn update_session_tag_and_duration_mid_round() {
        let conn = setup();
        let id = insert_session(&conn, "work", 1500, None).unwrap();
        update_session_tag(&conn, id, Some("  Rust  ")).unwrap();
        update_session_duration(&conn, id, 1800).unwrap();

        let (dur, tag): (u32, Option<String>) = conn
            .query_row(
                "SELECT duration_secs, tag FROM sessions WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dur, 1800);
        assert_eq!(tag.as_deref(), Some("Rust"));
    }

    #[test]
    fn profiles_crud() {
        let conn = setup();
        let initial = list_profiles(&conn).unwrap();
        assert_eq!(initial.len(), 3);

        let created = create_profile(
            &conn,
            &ProfileInput {
                name: "Mi bloque 45".to_string(),
                time_work_secs: 2700,
                time_short_break_secs: 600,
                time_long_break_secs: 1200,
                long_break_interval: 3,
                short_breaks_enabled: true,
                long_breaks_enabled: true,
            },
        )
        .unwrap();
        assert_eq!(created.name, "Mi bloque 45");
        assert_eq!(created.time_work_secs, 2700);
        assert!(!created.is_builtin);

        let updated = update_profile(
            &conn,
            created.id,
            &ProfileInput {
                name: "Bloque 45/10".to_string(),
                time_work_secs: 2700,
                time_short_break_secs: 600,
                time_long_break_secs: 1500,
                long_break_interval: 4,
                short_breaks_enabled: true,
                long_breaks_enabled: false,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Bloque 45/10");
        assert!(!updated.long_breaks_enabled);

        delete_profile(&conn, created.id).unwrap();
        assert!(get_profile(&conn, created.id).unwrap().is_none());
    }

    #[test]
    fn tags_crud_and_stats_filtering() {
        let conn = setup();
        let t1 = upsert_tag(&conn, "Facultad", None).unwrap();
        assert_eq!(t1.name, "Facultad");
        // Case-insensitive upsert reuses existing row.
        let t1_again = upsert_tag(&conn, "facultad", None).unwrap();
        assert_eq!(t1.id, t1_again.id);

        let s1 = insert_session(&conn, "work", 1500, Some("Facultad")).unwrap();
        complete_session(&conn, s1, true).unwrap();
        let s2 = insert_session(&conn, "work", 1800, Some("Código")).unwrap();
        complete_session(&conn, s2, true).unwrap();
        let s3 = insert_session(&conn, "work", 900, None).unwrap();
        complete_session(&conn, s3, true).unwrap();

        let all = get_daily_stats(&conn, TagFilter::All).unwrap();
        assert_eq!(all.rounds, 3);
        assert_eq!(all.focus_mins, 70);
        assert_eq!(all.by_tag.len(), 3);

        let fac = get_daily_stats(&conn, TagFilter::Tag("facultad")).unwrap();
        assert_eq!(fac.rounds, 1);
        assert_eq!(fac.focus_mins, 25);

        let untagged = get_daily_stats(&conn, TagFilter::Untagged).unwrap();
        assert_eq!(untagged.rounds, 1);
        assert_eq!(untagged.focus_mins, 15);

        let (used, has_untagged) = list_used_session_tags(&conn).unwrap();
        assert!(has_untagged);
        assert!(used.iter().any(|x| x == "Facultad"));
        assert!(used.iter().any(|x| x == "Código"));

        delete_tag(&conn, t1.id).unwrap();
        assert!(list_tags(&conn).unwrap().is_empty());
    }

    #[test]
    fn stats_empty_db() {
        let conn = setup();
        let stats = get_all_time_stats(&conn, TagFilter::All).unwrap();
        assert_eq!(stats.total_work_sessions, 0);
        assert_eq!(stats.completed_work_sessions, 0);
        assert_eq!(stats.total_work_secs, 0);
    }

    #[test]
    fn compute_streak_empty() {
        let info = compute_streak(&[], "2024-03-15");
        assert_eq!(info.current, 0);
        assert_eq!(info.longest, 0);
    }

    #[test]
    fn compute_streak_active_today() {
        let days = vec![
            "2024-03-13".to_string(),
            "2024-03-14".to_string(),
            "2024-03-15".to_string(),
        ];
        let info = compute_streak(&days, "2024-03-15");
        assert_eq!(info.current, 3);
        assert_eq!(info.longest, 3);
    }

    #[test]
    fn compute_streak_active_until_midnight() {
        let days = vec!["2024-03-13".to_string(), "2024-03-14".to_string()];
        let info = compute_streak(&days, "2024-03-15");
        assert_eq!(info.current, 2);
    }

    #[test]
    fn compute_streak_broken() {
        let days = vec!["2024-03-12".to_string(), "2024-03-13".to_string()];
        let info = compute_streak(&days, "2024-03-15");
        assert_eq!(info.current, 0);
    }

    #[test]
    fn compute_streak_longest_across_break() {
        let days = vec![
            "2024-03-01".to_string(),
            "2024-03-02".to_string(),
            "2024-03-03".to_string(),
            "2024-03-10".to_string(),
            "2024-03-11".to_string(),
        ];
        let info = compute_streak(&days, "2024-03-11");
        assert_eq!(info.current, 2);
        assert_eq!(info.longest, 3);
    }

    #[test]
    fn get_daily_stats_empty() {
        let conn = setup();
        let stats = get_daily_stats(&conn, TagFilter::All).unwrap();
        assert_eq!(stats.rounds, 0);
        assert_eq!(stats.focus_mins, 0);
        assert!(stats.completion_rate.is_none());
        assert_eq!(stats.by_hour.len(), 24);
    }

    #[test]
    fn get_weekly_stats_empty() {
        let conn = setup();
        let stats = get_weekly_stats(&conn, TagFilter::All).unwrap();
        assert!(stats.is_empty());
    }

    #[test]
    fn get_heatmap_data_empty() {
        let conn = setup();
        let entries = get_heatmap_data(&conn, TagFilter::All).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn focus_mins_rounds_to_nearest_minute() {
        let conn = setup();

        let id1 = insert_session(&conn, "work", 339, None).unwrap();
        complete_session(&conn, id1, true).unwrap();
        let stats = get_daily_stats(&conn, TagFilter::All).unwrap();
        assert_eq!(stats.focus_mins, 6, "339 s should round to 6 min");

        let conn2 = setup();
        let id2 = insert_session(&conn2, "work", 324, None).unwrap();
        complete_session(&conn2, id2, true).unwrap();
        let stats2 = get_daily_stats(&conn2, TagFilter::All).unwrap();
        assert_eq!(stats2.focus_mins, 5, "324 s should round to 5 min");

        let conn3 = setup();
        let id3 = insert_session(&conn3, "work", 1500, None).unwrap();
        complete_session(&conn3, id3, true).unwrap();
        let stats3 = get_daily_stats(&conn3, TagFilter::All).unwrap();
        assert_eq!(stats3.focus_mins, 25, "1500 s should be exactly 25 min");
    }

    #[test]
    fn stats_counts_correctly() {
        let conn = setup();

        let id1 = insert_session(&conn, "work", 1500, None).unwrap();
        complete_session(&conn, id1, true).unwrap();

        let id2 = insert_session(&conn, "work", 1500, None).unwrap();
        complete_session(&conn, id2, false).unwrap();

        let _id3 = insert_session(&conn, "short-break", 300, None).unwrap();

        let stats = get_all_time_stats(&conn, TagFilter::All).unwrap();
        assert_eq!(stats.total_work_sessions, 2);
        assert_eq!(stats.completed_work_sessions, 1);
        assert_eq!(stats.total_work_secs, 1500);
    }
}
