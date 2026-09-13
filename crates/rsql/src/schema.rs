//! What a filter is allowed to mention, and how each field reaches SQL.
//!
//! A filter arrives as user input, so nothing in it may ever be interpolated
//! into SQL. A field is only usable if it appears here, and it contributes a
//! *fixed* column expression written by us; every value the user typed leaves
//! as a bound parameter. That is the whole injection story.
//!
//! The field set mirrors rockbox's library schema — `track.length` is
//! milliseconds, `track.path` is the file path, likes live in `favourites`,
//! and play counters in `track_stats` (see the playlists crate).

/// How a field's values are compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Integer,
    /// Stored as a number but written as `true` / `false` / `yes` / `1`.
    Boolean,
    /// A timestamp column. Accepts an ISO date, or a relative age like `30d`
    /// / `6m` / `1y` which resolves against "now" at compile time.
    Timestamp,
}

/// One filterable field: the name a user writes, and the SQL behind it.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: &'static str,
    /// The SQL expression this field compares against. Written by us, never
    /// by the user — see the module docs.
    pub column: &'static str,
    pub kind: FieldKind,
    /// A short description, surfaced by the UIs that offer field pickers.
    pub label: &'static str,
}

impl Field {
    const fn new(
        name: &'static str,
        column: &'static str,
        kind: FieldKind,
        label: &'static str,
    ) -> Self {
        Self {
            name,
            column,
            kind,
            label,
        }
    }
}

/// The set of fields one kind of query may filter on, plus the FROM clause
/// that makes those columns resolvable.
#[derive(Clone, Debug)]
pub struct Schema {
    pub name: &'static str,
    /// `FROM` body — the base table and any joins the fields depend on.
    pub from: &'static str,
    /// The identifier column the query selects.
    pub id_column: &'static str,
    pub fields: &'static [Field],
}

impl Schema {
    pub fn field(&self, name: &str) -> Option<&Field> {
        let name = name.to_ascii_lowercase();
        self.fields.iter().find(|f| f.name == name)
    }

    /// Field names, for "did you mean" hints and UI pickers.
    pub fn field_names(&self) -> Vec<&'static str> {
        self.fields.iter().map(|f| f.name).collect()
    }
}

/// Tracks, with album, genre and play counters joined in so a filter can say
/// `album==...` or `playcount>5` without the caller knowing the schema.
///
/// `track_stats` is a LEFT JOIN: a track nobody has played yet has no row
/// there, and `playcount==0` still has to match it — hence the COALESCE.
pub const TRACKS: Schema = Schema {
    name: "tracks",
    from: "track \
           LEFT JOIN album ON album.id = track.album_id \
           LEFT JOIN genre ON genre.id = track.genre_id \
           LEFT JOIN track_stats ON track_stats.track_id = track.id",
    id_column: "track.id",
    fields: &[
        Field::new("title", "track.title", FieldKind::Text, "Title"),
        Field::new("artist", "track.artist", FieldKind::Text, "Artist"),
        Field::new("album", "track.album", FieldKind::Text, "Album"),
        Field::new(
            "albumartist",
            "track.album_artist",
            FieldKind::Text,
            "Album artist",
        ),
        Field::new("genre", "genre.name", FieldKind::Text, "Genre"),
        Field::new("year", "track.year", FieldKind::Integer, "Year"),
        Field::new(
            "track",
            "track.track_number",
            FieldKind::Integer,
            "Track number",
        ),
        Field::new("disc", "track.disc_number", FieldKind::Integer, "Disc"),
        // Milliseconds in the column; filters are written in seconds, which is
        // how a human thinks about track length (`duration>300` = over five
        // minutes), so the column is scaled here rather than in every filter.
        Field::new(
            "duration",
            "(track.length / 1000)",
            FieldKind::Integer,
            "Duration (seconds)",
        ),
        Field::new("bitrate", "track.bitrate", FieldKind::Integer, "Bitrate"),
        Field::new(
            "samplerate",
            "track.frequency",
            FieldKind::Integer,
            "Sample rate",
        ),
        // Traditional notation — `Fm`, `D` — matched case-insensitively like
        // any other text field, so `key==fm` works. Null until the track has
        // been analysed, which `key=null=` selects for.
        Field::new("key", "track.key", FieldKind::Text, "Musical key"),
        // Integer rather than a decimal: tempo is read, sorted and filtered as
        // a whole number, and `bpm>=120` should not miss a track stored as
        // 119.97.
        Field::new(
            "bpm",
            "CAST(ROUND(track.bpm) AS INTEGER)",
            FieldKind::Integer,
            "Tempo (BPM)",
        ),
        Field::new("path", "track.path", FieldKind::Text, "File path"),
        Field::new(
            "liked",
            "EXISTS (SELECT 1 FROM favourites WHERE favourites.track_id = track.id)",
            FieldKind::Boolean,
            "Liked",
        ),
        Field::new(
            "playcount",
            "COALESCE(track_stats.play_count, 0)",
            FieldKind::Integer,
            "Play count",
        ),
        Field::new(
            "skipcount",
            "COALESCE(track_stats.skip_count, 0)",
            FieldKind::Integer,
            "Skip count",
        ),
        Field::new(
            "lastplayed",
            "track_stats.last_played",
            FieldKind::Timestamp,
            "Last played",
        ),
        Field::new(
            "lastskipped",
            "track_stats.last_skipped",
            FieldKind::Timestamp,
            "Last skipped",
        ),
        // ISO-8601 in the column, unix seconds in the comparison — converted
        // here so `added=gt=30d` works like every other timestamp field.
        Field::new(
            "added",
            "unixepoch(track.created_at)",
            FieldKind::Timestamp,
            "Date added",
        ),
    ],
};

pub const ALBUMS: Schema = Schema {
    name: "albums",
    from: "album",
    id_column: "album.id",
    fields: &[
        Field::new("title", "album.title", FieldKind::Text, "Title"),
        Field::new("artist", "album.artist", FieldKind::Text, "Artist"),
        Field::new("year", "album.year", FieldKind::Integer, "Year"),
        Field::new("label", "album.label", FieldKind::Text, "Label"),
    ],
};

pub const ARTISTS: Schema = Schema {
    name: "artists",
    from: "artist",
    id_column: "artist.id",
    fields: &[Field::new("name", "artist.name", FieldKind::Text, "Name")],
};

/// Every schema, for callers that resolve one by name (the HTTP filter
/// endpoints and the GraphQL layer both do).
pub const ALL: &[&Schema] = &[&TRACKS, &ALBUMS, &ARTISTS];

/// Look a schema up by the name a caller passes in ("tracks", "albums", …).
pub fn by_name(name: &str) -> Option<&'static Schema> {
    let name = name.to_ascii_lowercase();
    ALL.iter().copied().find(|schema| schema.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_resolve_case_insensitively() {
        assert!(TRACKS.field("Artist").is_some());
        assert!(TRACKS.field("PLAYCOUNT").is_some());
        assert!(TRACKS.field("nope").is_none());
    }

    #[test]
    fn schemas_resolve_by_name() {
        assert!(by_name("tracks").is_some());
        assert!(by_name("Albums").is_some());
        assert!(by_name("radios").is_none());
    }
}
