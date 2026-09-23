-- Indexes over the fingerprint columns added in 20260923000000.
--
-- Idempotent by construction (IF NOT EXISTS), so this runs unconditionally on
-- every boot — unlike the ALTERs it follows, which fail once applied.

-- The duplicate lookup: "which other tracks are this recording?". Partial, so
-- the not-attempted and no-match rows — most of the table until the pass has
-- run — cost nothing to keep indexed.
CREATE INDEX IF NOT EXISTS idx_track_acoustid
  ON track (acoustid) WHERE acoustid IS NOT NULL AND acoustid != '';

-- The fingerprint pass's work queue, in the order it reads it: local tracks
-- that have no fingerprint yet.
CREATE INDEX IF NOT EXISTS idx_track_fingerprint_pending
  ON track (is_remote, fingerprint);

-- The identification stage's queue: fingerprinted, not yet looked up.
CREATE INDEX IF NOT EXISTS idx_track_acoustid_pending
  ON track (acoustid) WHERE acoustid IS NULL;
