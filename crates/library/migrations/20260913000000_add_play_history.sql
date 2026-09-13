-- Listening analytics.
--
-- `track_stats` already keeps the running counters (play_count, skip_count,
-- last_played, last_skipped), which is what smart-playlist rules and the
-- rsql `playcount` / `lastplayed` fields read. It cannot answer anything
-- about *when*: there is one row per track, overwritten in place, so a play
-- last night and a play last year are indistinguishable.
--
-- `play_history` is the event log those counters are a summary of. One row per
-- listen, appended and never updated, so questions like "what did I play this
-- week", "what am I wearing out", and "what have I not touched since March"
-- become ordinary queries. The counters stay: they are the fast path, and
-- rebuilding them from the log on every read would be wasteful.

CREATE TABLE IF NOT EXISTS play_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    track_id TEXT NOT NULL,
    -- Unix seconds, matching track_stats.last_played rather than the ISO
    -- strings the library tables use, so the two can be compared directly.
    played_at INTEGER NOT NULL,
    -- How much was actually heard, and how much there was to hear. Kept as
    -- recorded rather than as a ratio so a later change of mind about what
    -- counts as a skip can be applied to history already collected.
    ms_played INTEGER NOT NULL DEFAULT 0,
    length_ms INTEGER NOT NULL DEFAULT 0,
    -- 1 when this listen was counted as a skip, by whatever rule was in force
    -- at the time. Denormalised on purpose: see above.
    skipped INTEGER NOT NULL DEFAULT 0
);

-- "Everything for this track" and "everything since <date>" are the two access
-- patterns; the log grows without bound, so neither should be a table scan.
CREATE INDEX IF NOT EXISTS play_history_track_idx ON play_history(track_id);
CREATE INDEX IF NOT EXISTS play_history_played_at_idx ON play_history(played_at DESC);

-- ── Views ───────────────────────────────────────────────────────────────────
--
-- Kept in SQL rather than in Rust so every consumer sees the same definition:
-- the gRPC and GraphQL servers, the Subsonic and Jellyfin bridges, and anyone
-- who opens the file with the sqlite3 CLI.

-- Most played, by lifetime count. Reads the counters, not the log, so it
-- includes plays recorded before this table existed.
CREATE VIEW IF NOT EXISTS v_most_played AS
SELECT t.id AS track_id,
       t.title,
       t.artist,
       t.album,
       COALESCE(s.play_count, 0) AS play_count,
       s.last_played
FROM track t
JOIN track_stats s ON s.track_id = t.id
WHERE COALESCE(s.play_count, 0) > 0
ORDER BY s.play_count DESC, s.last_played DESC;

CREATE VIEW IF NOT EXISTS v_most_skipped AS
SELECT t.id AS track_id,
       t.title,
       t.artist,
       t.album,
       COALESCE(s.skip_count, 0) AS skip_count,
       s.last_skipped
FROM track t
JOIN track_stats s ON s.track_id = t.id
WHERE COALESCE(s.skip_count, 0) > 0
ORDER BY s.skip_count DESC, s.last_skipped DESC;

-- Never played: no counter row at all, or one that has only ever recorded
-- skips. A track skipped ten times has still never been listened to.
CREATE VIEW IF NOT EXISTS v_never_played AS
SELECT t.id AS track_id,
       t.title,
       t.artist,
       t.album,
       t.created_at
FROM track t
LEFT JOIN track_stats s ON s.track_id = t.id
WHERE COALESCE(s.play_count, 0) = 0
ORDER BY t.created_at DESC;

-- Recently added, from the scan timestamp. `created_at` is an ISO string here,
-- which sorts correctly as text because it is zero-padded and UTC.
CREATE VIEW IF NOT EXISTS v_recently_added AS
SELECT t.id AS track_id,
       t.title,
       t.artist,
       t.album,
       t.created_at
FROM track t
ORDER BY t.created_at DESC;

-- Recently played, from the log: one row per listen, newest first. Distinct
-- from `v_most_played` in that a track appears once per time it was played.
CREATE VIEW IF NOT EXISTS v_recently_played AS
SELECT h.id,
       h.track_id,
       t.title,
       t.artist,
       t.album,
       h.played_at,
       h.ms_played,
       h.length_ms,
       h.skipped
FROM play_history h
JOIN track t ON t.id = h.track_id
ORDER BY h.played_at DESC;
