package org.rockbox.ffi

// Note the two *different* ReplayGain encodings in the C ABI.

/** Values for [Dsp.setReplaygain] (native Rockbox encoding). */
enum class DspReplayGainMode(val value: Int) {
    TRACK(0),
    ALBUM(1),
    SHUFFLE(2),
    OFF(3),
}

/** Values for [Player.setReplaygain] and [Player.Config.replaygainMode]. */
enum class ReplayGainMode(val value: Int) {
    OFF(0),
    TRACK(1),
    ALBUM(2),
}

enum class CrossfadeMode(val value: Int) {
    OFF(0),
    AUTO_SKIP(1),
    MANUAL_SKIP(2),
    SHUFFLE(3),
    SHUFFLE_OR_MANUAL(4),
    ALWAYS(5),
}

enum class MixMode(val value: Int) {
    CROSSFADE(0),
    MIX(1),
}

/** Repeat mode for [Player.setRepeat] / [Player.repeat] (`rb_player_set_repeat`). */
enum class RepeatMode(val value: Int) {
    OFF(0),
    ONE(1),
    ALL(2),
}

/** Where to place tracks when inserting into the queue (`rb_player_insert_json`). */
enum class InsertPosition(val value: Int) {
    PREPEND(0),
    INSERT(1),
    INSERT_NEXT(2),
    INSERT_LAST(3),
    INSERT_SHUFFLED(4),
    INSERT_LAST_SHUFFLED(5),
    REPLACE(6),
    INDEX(7),
}

enum class ChannelConfig(val value: Int) {
    STEREO(0),
    MONO(1),
    CUSTOM(2),
    MONO_LEFT(3),
    MONO_RIGHT(4),
    KARAOKE(5),
    SWAP(6),
}

/** Built-in EQ presets for [Player.setEqPreset] (`rb_player_set_eq_preset`). */
enum class EqPreset(val value: Int) {
    FLAT(0),
    ACOUSTIC(1),
    BASS_BOOST(2),
    BASS_REDUCER(3),
    CLASSICAL(4),
    DANCE(5),
    DEEP(6),
    ELECTRONIC(7),
    HIP_HOP(8),
    JAZZ(9),
    LATIN(10),
    LOUDNESS(11),
    LOUNGE(12),
    PIANO(13),
    POP(14),
    RNB(15),
    ROCK(16),
    SMALL_SPEAKERS(17),
    TREBLE_BOOST(18),
    TREBLE_REDUCER(19),
    VOCAL_BOOST(20),
}

/** Channel mixing modes for [Player.setChannelMode] (`rb_player_set_channel_mode`). */
enum class ChannelMode(val value: Int) {
    STEREO(0),
    MONO(1),
    CUSTOM(2),
    MONO_LEFT(3),
    MONO_RIGHT(4),
    KARAOKE(5),
    SWAP(6),
}
