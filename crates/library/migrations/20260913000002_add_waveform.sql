-- Per-track waveform peaks, written by the same analysis pass that fills
-- key/bpm: 400 bytes, one peak per bin, drawn by the desktop's seek bar.
-- NULL until analysed; a track whose decode failed stays NULL and the UI
-- falls back to a flat rail.
ALTER TABLE track ADD COLUMN waveform BLOB;
