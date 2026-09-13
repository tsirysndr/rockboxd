-- Musical key and tempo, written by the analysis pass (crates/library's
-- analyse module) and read by rsql filters (`key==Am`, `bpm>=120`) and smart
-- playlists.
--
-- `key` is traditional notation ("F", "Am"), matched case-insensitively.
-- `bpm` is a REAL; filters round it. Both NULL until the track is analysed —
-- deliberately distinguishable from "analysed, found nothing".
ALTER TABLE track ADD COLUMN key TEXT;
ALTER TABLE track ADD COLUMN bpm REAL;
