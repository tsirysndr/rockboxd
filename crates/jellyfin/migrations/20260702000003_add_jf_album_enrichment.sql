-- Cache table for Last.fm `album.getInfo` responses. Mirrors
-- `jf_artist_enrichment`: cache-only reads populate `Overview` /
-- `Genres` on every `album_to_dto` call; the detail handlers
-- (`/Items/{album_guid}`, `/Users/{uid}/Items/{album_guid}`) refresh
-- from Last.fm on demand when the row is missing or older than the
-- module's TTL.
CREATE TABLE IF NOT EXISTS jf_album_enrichment (
    album_id    TEXT PRIMARY KEY,    -- rockbox-library album id
    mbid        TEXT,                -- MusicBrainz release / release-group id
    description TEXT,                -- wiki summary / content, cleaned
    tags        TEXT,                -- JSON-encoded Vec<String>
    image_url   TEXT,                -- Last.fm cover URL (may be a placeholder)
    fetched_at  INTEGER NOT NULL     -- unix seconds
);
