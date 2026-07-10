import Foundation

/// Audio file metadata parsing.
public enum Metadata {
    /// Parse the metadata of the audio file at `path`.
    ///
    /// Returns a dictionary with tag strings, `duration_ms`, `bitrate`,
    /// `sample_rate`, a nested `replaygain` dictionary (including the raw
    /// Q7.24 `raw_*` fields), and optional `album_art` / `cuesheet`
    /// locations. Throws on failure.
    public static func read(_ path: String) throws -> [String: Any] {
        let lib = Lib.shared
        let raw = path.withCString { lib.metaReadJson($0) }
        guard let json = lib.takeString(raw) else {
            throw RockboxError.nullReturn("rb_meta_read_json(\(path))")
        }
        return try parseObject(json, context: "metadata")
    }

    /// Guess the codec label (e.g. "FLAC") from a filename's extension
    /// without opening the file. Returns nil for an unknown extension.
    public static func probe(_ filename: String) -> String? {
        let lib = Lib.shared
        let raw = filename.withCString { lib.metaProbe($0) }
        return lib.takeString(raw)
    }
}

/// Parse a JSON object string into a dictionary.
func parseObject(_ json: String, context: String) throws -> [String: Any] {
    guard let data = json.data(using: .utf8),
          let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
        throw RockboxError.invalidInput("could not parse \(context) JSON")
    }
    return obj
}
