pub const std = @import("std");

pub const EQ_NUM_BANDS = 10;
pub const QUICKSCREEN_ITEM_COUNT = 4;
pub const MAX_FILENAME = 32;
pub const MAX_PATHNAME = 256;

pub const ReplaygainSettings = extern struct {
    noclip: bool,
    type: c_int,
    preamp: c_int,
};

pub const EqBandSetting = extern struct {
    type: c_int,
    cutoff: c_int,
    q: c_int,
    gain: c_int,
};

test "EQ band matches the C layout" {
    try std.testing.expectEqual(@as(usize, 16), @sizeOf(EqBandSetting));
    try std.testing.expectEqual(@as(usize, 0), @offsetOf(EqBandSetting, "type"));
    try std.testing.expectEqual(@as(usize, 4), @offsetOf(EqBandSetting, "cutoff"));
    try std.testing.expectEqual(@as(usize, 8), @offsetOf(EqBandSetting, "q"));
    try std.testing.expectEqual(@as(usize, 12), @offsetOf(EqBandSetting, "gain"));
}

pub const StorageType = extern union { int_val: c_int };

pub const SoundSetting = extern struct { setting: c_int };

pub const BoolSetting = extern struct { lang_yes: c_int, lang_no: c_int };

pub const FilenameSetting = extern struct {
    prefix: [*]const u8,
    suffix: [*]const u8,
    max_len: c_int,
};

pub const IntSetting = extern struct {
    unit: c_int,
    step: c_int,
    min: c_int,
    max: c_int,
};

pub const ChoiceSettingData = extern struct {
    desc: [*]const [*]const u8,
    talks: [*]const c_int,
};

pub const ChoiceSetting = extern struct {
    count: c_int,
    data: ChoiceSettingData,
};

pub const TableSetting = extern struct {
    unit: c_int,
    count: c_int,
    values: [*]const c_int,
};

pub const CustomSetting = extern struct {
    unit: c_int,
    count: c_int,
    values: [*]const c_int,
};

pub const SettingsTypeUnion = extern union {
    sound_setting: SoundSetting,
    bool_setting: BoolSetting,
    filename_setting: FilenameSetting,
    int_setting: IntSetting,
    choice_setting: ChoiceSetting,
    table_setting: TableSetting,
    custom_setting: CustomSetting,
};

pub const SettingsList = extern struct {
    flags: c_uint,
    lang_id: c_int,
    default_val: StorageType,
    cfg_name: [*]const u8,
    cfg_vals: [*]const u8,
    settings_type: SettingsTypeUnion,
};

pub const TouchscreenParameter = extern struct {
    A: c_int,
    B: c_int,
    C: c_int,
    D: c_int,
    E: c_int,
    F: c_int,
    divider: c_int,
};

pub const CompressorSettings = extern struct {
    threshold: c_int,
    makeup_gain: c_int,
    ratio: c_int,
    knee: c_int,
    release_time: c_int,
    attack_time: c_int,
};

pub const UserSettings = extern struct {
    // Audio settings
    volume: c_int,
    balance: c_int,
    bass: c_int,
    treble: c_int,
    channel_config: c_int,
    stereo_width: c_int,

    bass_cutoff: c_int,
    treble_cutoff: c_int,

    crossfade: c_int,
    crossfade_fade_in_delay: c_int,
    crossfade_fade_out_delay: c_int,
    crossfade_fade_in_duration: c_int,
    crossfade_fade_out_duration: c_int,
    crossfade_fade_out_mixmode: c_int,

    // Replaygain
    replaygain_settings: ReplaygainSettings,

    // Crossfeed
    crossfeed: c_int,
    crossfeed_direct_gain: c_uint,
    crossfeed_cross_gain: c_uint,
    crossfeed_hf_attenuation: c_uint,
    crossfeed_hf_cutoff: c_uint,

    // EQ
    eq_enabled: u8,
    eq_precut: c_uint,
    eq_band_settings: [EQ_NUM_BANDS]EqBandSetting,

    // Misc. swcodec
    beep: c_int,
    keyclick: c_int,
    keyclick_repeats: c_int,
    dithering_enabled: u8,
    timestretch_enabled: u8,

    // Misc options
    list_accel_start_delay: c_int,
    list_accel_wait: c_int,

    touchpad_sensitivity: c_int,
    touchpad_deadzone: c_int,

    pause_rewind: c_int,
    unplug_mode: c_int,
    unplug_autoresume: u8,

    qs_items: [QUICKSCREEN_ITEM_COUNT]SettingsList,

    timeformat: c_int,
    disk_spindown: c_int,
    buffer_margin: c_int,

    dirfilter: c_int,
    show_filename_ext: c_int,
    default_codepage: c_int,
    hold_lr_for_scroll_in_list: u8,
    play_selected: u8,
    single_mode: c_int,
    party_mode: u8,
    cuesheet: u8,
    car_adapter_mode: u8,
    car_adapter_mode_delay: c_int,
    start_in_screen: c_int,
    ff_rewind_min_step: c_int,
    ff_rewind_accel: c_int,

    peak_meter_release: c_int,
    peak_meter_hold: c_int,
    peak_meter_clip_hold: c_int,
    peak_meter_dbfs: u8,
    peak_meter_min: c_int,
    peak_meter_max: c_int,

    wps_file: [MAX_FILENAME + 1]u8,
    sbs_file: [MAX_FILENAME + 1]u8,
    lang_file: [MAX_FILENAME + 1]u8,
    playlist_catalog_dir: [MAX_PATHNAME + 1]u8,
    skip_length: c_int,
    max_files_in_dir: c_int,
    max_files_in_playlist: c_int,
    volume_type: c_int,
    battery_display: c_int,
    show_icons: u8,
    statusbar: c_int,

    scrollbar: c_int,
    scrollbar_width: c_int,

    list_line_padding: c_int,
    list_separator_height: c_int,
    list_separator_color: c_int,

    browse_current: u8,
    scroll_paginated: u8,
    list_wraparound: u8,
    list_order: c_int,
    scroll_speed: c_int,
    bidir_limit: c_int,
    scroll_delay: c_int,
    scroll_step: c_int,

    autoloadbookmark: c_int,
    autocreatebookmark: c_int,
    autoupdatebookmark: u8,
    usemrb: c_int,

    dircache: u8,
    tagcache_ram: c_int,
    tagcache_autoupdate: u8,
    autoresume_enable: u8,
    autoresume_automatic: c_int,
    autoresume_paths: [MAX_PATHNAME + 1]u8,
    runtimedb: u8,
    tagcache_scan_paths: [MAX_PATHNAME + 1]u8,
    tagcache_db_path: [MAX_PATHNAME + 1]u8,
    backdrop_file: [MAX_PATHNAME + 1]u8,

    bg_color: c_int,
    fg_color: c_int,
    lss_color: c_int,
    lse_color: c_int,
    lst_color: c_int,
    colors_file: [MAX_FILENAME + 1]u8,

    browser_default: c_int,

    repeat_mode: c_int,
    next_folder: c_int,
    constrain_next_folder: u8,
    recursive_dir_insert: c_int,
    fade_on_stop: u8,
    playlist_shuffle: u8,
    warnon_erase_dynplaylist: u8,
    keep_current_track_on_replace_playlist: u8,
    show_shuffled_adding_options: u8,
    show_queue_options: c_int,
    album_art: c_int,
    rewind_across_tracks: u8,

    playlist_viewer_icons: u8,
    playlist_viewer_indices: u8,
    playlist_viewer_track_display: c_int,

    talk_menu: u8,
    talk_dir: c_int,
    talk_dir_clip: u8,
    talk_file: c_int,
    talk_file_clip: u8,
    talk_filetype: u8,
    talk_battery_level: u8,
    talk_mixer_amp: c_int,

    sort_case: u8,
    sort_dir: c_int,
    sort_file: c_int,
    interpret_numbers: c_int,

    poweroff: c_int,
    battery_capacity: c_int,
    battery_type: c_int,
    spdif_enable: u8,
    usb_charging: c_int,

    contrast: c_int,
    invert: u8,
    flip_display: u8,
    cursor_style: c_int,
    screen_scroll_step: c_int,
    show_path_in_browser: c_int,
    offset_out_of_view: u8,
    disable_mainmenu_scrolling: u8,
    icon_file: [MAX_FILENAME + 1]u8,
    viewers_icon_file: [MAX_FILENAME + 1]u8,
    font_file: [MAX_FILENAME + 1]u8,
    glyphs_to_cache: c_int,
    kbd_file: [MAX_FILENAME + 1]u8,
    backlight_timeout: c_int,
    caption_backlight: u8,
    bl_filter_first_keypress: u8,
    backlight_timeout_plugged: c_int,
    bt_selective_softlock_actions: u8,
    bt_selective_softlock_actions_mask: c_int,
    bl_selective_actions: u8,
    bl_selective_actions_mask: c_int,
    backlight_on_button_hold: c_int,
    lcd_sleep_after_backlight_off: c_int,
    brightness: c_int,

    speaker_mode: c_int,
    prevent_skip: u8,

    touch_mode: c_int,
    ts_calibration_data: TouchscreenParameter,

    pitch_mode_semitone: u8,
    pitch_mode_timestretch: u8,

    usb_hid: u8,
    usb_keypad_mode: c_int,

    usb_skip_first_drive: u8,

    ui_vp_config: [64]u8,
    player_name: [64]u8,

    compressor_settings: CompressorSettings,

    sleeptimer_duration: c_int,
    sleeptimer_on_startup: u8,
    keypress_restarts_sleeptimer: u8,

    show_shutdown_message: u8,

    hotkey_wps: c_int,
    hotkey_tree: c_int,

    resume_rewind: c_int,

    depth_3d: c_int,

    roll_off: c_int,

    power_mode: c_int,

    keyclick_hardware: u8,

    start_directory: [MAX_PATHNAME + 1]u8,
    root_menu_customized: u8,
    shortcuts_replaces_qs: u8,

    play_frequency: c_int,
    volume_limit: c_int,

    volume_adjust_mode: c_int,
    volume_adjust_norm_steps: c_int,

    surround_enabled: c_int,
    surround_balance: c_int,
    surround_fx1: c_int,
    surround_fx2: u8,
    surround_method2: u8,
    surround_mix: c_int,

    pbe: c_int,
    pbe_precut: c_int,

    afr_enabled: c_int,

    governor: c_int,
    stereosw_mode: c_int,
};

extern var global_settings: UserSettings;

pub fn get_crossfade_mode() c_int {
    return global_settings.crossfade;
}
