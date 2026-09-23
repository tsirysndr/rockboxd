-- Acoustic fingerprints and what they identify.
--
-- `fingerprint` is a Chromaprint TEST2 fingerprint, compressed and base64url
-- encoded — the same string `fpcalc` prints. `fingerprint_duration` is the
-- whole track's length in seconds as the decoder measured it, which AcoustID
-- matches on alongside the fingerprint itself.
--
-- `acoustid` is the AcoustID the fingerprint resolved to: one id per
-- recording, stable across encodings, so two rows sharing one are the same
-- music in different files. `musicbrainz_recording_id` is the MBID behind it,
-- kept so a later lookup can go straight to MusicBrainz.
--
-- NULL means "not attempted yet" and drives the background pass's work queue.
-- The empty string means "attempted, nothing came back" — a file that cannot
-- be decoded, or a recording AcoustID has never seen — and keeps the pass from
-- retrying the same dead end on every boot. The two are deliberately
-- distinguishable.
--
-- Plain ALTERs, so this whole file fails harmlessly once applied. The indexes
-- over these columns live in 20260923000001 for that reason — a failed ALTER
-- aborts the rest of its own script, and the indexes must not be collateral.
ALTER TABLE track ADD COLUMN fingerprint TEXT;
ALTER TABLE track ADD COLUMN fingerprint_duration REAL;
ALTER TABLE track ADD COLUMN acoustid TEXT;
ALTER TABLE track ADD COLUMN musicbrainz_recording_id TEXT;
