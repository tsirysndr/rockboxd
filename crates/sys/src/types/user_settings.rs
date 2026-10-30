use std::ffi::CStr;

use crate::cast_ptr;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ReplaygainSettings {
    pub noclip: bool, // scale to prevent clips
    pub r#type: i32, // 0=track gain, 1=album gain, 2=track gain if shuffle is on, album gain otherwise, 4=off
    pub preamp: i32, // scale replaygained tracks by this
}

impl From<crate::ReplaygainSettings> for ReplaygainSettings {
    fn from(settings: crate::ReplaygainSettings) -> Self {
        let noclip = settings.noclip;
        Self {
            noclip,
            r#type: settings.r#type,
            preamp: settings.preamp,
        }
    }
}

impl Into<crate::ReplaygainSettings> for ReplaygainSettings {
    fn into(self) -> crate::ReplaygainSettings {
        crate::ReplaygainSettings {
            noclip: self.noclip,
            r#type: self.r#type,
            preamp: self.preamp,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct EqBandSetting {
    pub cutoff: i32,
    pub q: i32,
    pub gain: i32,
}

impl From<crate::EqBandSetting> for EqBandSetting {
    fn from(setting: crate::EqBandSetting) -> Self {
        Self {
            cutoff: setting.cutoff,
            q: setting.q,
            gain: setting.gain,
        }
    }
}

impl EqBandSetting {
    pub fn into_raw(self, band: usize) -> crate::EqBandSetting {
        crate::EqBandSetting::new(band, self.cutoff, self.q, self.gain)
    }
}

#[derive(Serialize, Deserialize)]
pub struct TouchscreenParameter {
    pub A: i32,
    pub B: i32,
    pub C: i32,
    pub D: i32,
    pub E: i32,
    pub F: i32,
    pub divider: i32,
}

impl From<crate::TouchscreenParameter> for TouchscreenParameter {
    fn from(parameter: crate::TouchscreenParameter) -> Self {
        Self {
            A: parameter.A,
            B: parameter.B,
            C: parameter.C,
            D: parameter.D,
            E: parameter.E,
            F: parameter.F,
            divider: parameter.divider,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct CompressorSettings {
    pub threshold: i32,
    pub makeup_gain: i32,
    pub ratio: i32,
    pub knee: i32,
    pub release_time: i32,
    pub attack_time: i32,
}

impl From<crate::CompressorSettings> for CompressorSettings {
    fn from(settings: crate::CompressorSettings) -> Self {
        Self {
            threshold: settings.threshold,
            makeup_gain: settings.makeup_gain,
            ratio: settings.ratio,
            knee: settings.knee,
            release_time: settings.release_time,
            attack_time: settings.attack_time,
        }
    }
}

impl Into<crate::CompressorSettings> for CompressorSettings {
    fn into(self) -> crate::CompressorSettings {
        crate::CompressorSettings {
            threshold: self.threshold,
            makeup_gain: self.makeup_gain,
            ratio: self.ratio,
            knee: self.knee,
            release_time: self.release_time,
            attack_time: self.attack_time,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct UserSettings {
    pub music_dir: String,

    // Audio settings
    pub balance: i32,
    pub bass: i32,
    pub treble: i32,
    pub channel_config: i32,
    pub stereo_width: i32,

    pub bass_cutoff: i32,
    pub treble_cutoff: i32,

    pub crossfade: i32,
    pub crossfade_fade_in_delay: i32,
    pub crossfade_fade_out_delay: i32,
    pub crossfade_fade_in_duration: i32,
    pub crossfade_fade_out_duration: i32,
    pub crossfade_fade_out_mixmode: i32,

    // Replaygain
    pub replaygain_settings: ReplaygainSettings,

    // Crossfeed
    pub crossfeed: i32,
    pub crossfeed_direct_gain: i32,
    pub crossfeed_cross_gain: i32,
    pub crossfeed_hf_attenuation: i32,
    pub crossfeed_hf_cutoff: u32,

    // EQ
    pub eq_enabled: bool,
    pub eq_precut: u32,
    pub eq_band_settings: Vec<EqBandSetting>,

    // Misc. swcodec
    pub beep: i32,
    pub keyclick: i32,
    pub keyclick_repeats: i32,
    pub dithering_enabled: bool,
    pub timestretch_enabled: bool,

    // Misc options
    pub list_accel_start_delay: i32,
    pub list_accel_wait: i32,

    pub touchpad_sensitivity: i32,
    pub touchpad_deadzone: i32,

    pub pause_rewind: i32,
    pub unplug_mode: i32,
    pub unplug_autoresume: bool,

    pub timeformat: i32,
    pub disk_spindown: i32,
    pub buffer_margin: i32,

    pub dirfilter: i32,
    pub show_filename_ext: i32,
    pub default_codepage: i32,
    pub hold_lr_for_scroll_in_list: bool,
    pub play_selected: bool,
    pub single_mode: i32,
    pub party_mode: bool,
    pub cuesheet: bool,
    pub car_adapter_mode: bool,
    pub car_adapter_mode_delay: i32,
    pub start_in_screen: i32,
    pub ff_rewind_min_step: i32,
    pub ff_rewind_accel: i32,

    pub peak_meter_release: i32,
    pub peak_meter_hold: i32,
    pub peak_meter_clip_hold: i32,
    pub peak_meter_dbfs: bool,
    pub peak_meter_min: i32,
    pub peak_meter_max: i32,

    pub wps_file: String,
    pub sbs_file: String,
    pub lang_file: String,
    pub playlist_catalog_dir: String,
    pub skip_length: i32,
    pub max_files_in_dir: i32,
    pub max_files_in_playlist: i32,
    pub volume_type: i32,
    pub battery_display: i32,
    pub show_icons: bool,
    pub statusbar: i32,

    pub scrollbar: i32,
    pub scrollbar_width: i32,

    pub list_line_padding: i32,
    pub list_separator_height: i32,
    pub list_separator_color: i32,

    pub browse_current: bool,
    pub scroll_paginated: bool,
    pub list_wraparound: bool,
    pub list_order: i32,
    pub scroll_speed: i32,
    pub bidir_limit: i32,
    pub scroll_delay: i32,
    pub scroll_step: i32,

    pub autoloadbookmark: i32,
    pub autocreatebookmark: i32,
    pub autoupdatebookmark: bool,
    pub usemrb: i32,

    pub dircache: bool,
    pub tagcache_ram: i32,
    pub tagcache_autoupdate: bool,
    pub autoresume_enable: bool,
    pub autoresume_automatic: i32,
    pub autoresume_paths: String,
    pub runtimedb: bool,
    pub tagcache_scan_paths: String,
    pub tagcache_db_path: String,
    pub backdrop_file: String,

    pub bg_color: i32,
    pub fg_color: i32,
    pub lss_color: i32,
    pub lse_color: i32,
    pub lst_color: i32,
    pub colors_file: String,

    pub browser_default: i32,

    pub repeat_mode: i32,
    pub next_folder: i32,
    pub constrain_next_folder: bool,
    pub recursive_dir_insert: i32,
    pub fade_on_stop: bool,
    pub playlist_shuffle: bool,
    pub warnon_erase_dynplaylist: bool,
    pub keep_current_track_on_replace_playlist: bool,
    pub show_shuffled_adding_options: bool,
    pub show_queue_options: i32,
    pub album_art: i32,
    pub rewind_across_tracks: bool,

    pub playlist_viewer_icons: bool,
    pub playlist_viewer_indices: bool,
    pub playlist_viewer_track_display: i32,

    pub talk_menu: bool,
    pub talk_dir: i32,
    pub talk_dir_clip: bool,
    pub talk_file: i32,
    pub talk_file_clip: bool,
    pub talk_filetype: bool,
    pub talk_battery_level: bool,
    pub talk_mixer_amp: i32,

    pub sort_case: bool,
    pub sort_dir: i32,
    pub sort_file: i32,
    pub interpret_numbers: i32,

    pub poweroff: i32,
    pub battery_capacity: i32,
    pub battery_type: i32,
    pub spdif_enable: bool,
    pub usb_charging: i32,

    pub contrast: i32,
    pub invert: bool,
    pub flip_display: bool,
    pub cursor_style: i32,
    pub screen_scroll_step: i32,
    pub show_path_in_browser: i32,
    pub offset_out_of_view: bool,
    pub disable_mainmenu_scrolling: bool,
    pub icon_file: String,
    pub viewers_icon_file: String,
    pub font_file: String,
    pub glyphs_to_cache: i32,
    pub kbd_file: String,
    pub backlight_timeout: i32,
    pub caption_backlight: bool,
    pub bl_filter_first_keypress: bool,
    pub backlight_timeout_plugged: i32,
    pub bt_selective_softlock_actions: bool,
    pub bt_selective_softlock_actions_mask: i32,
    pub bl_selective_actions: bool,
    pub bl_selective_actions_mask: i32,
    pub backlight_on_button_hold: i32,
    pub lcd_sleep_after_backlight_off: i32,
    pub brightness: i32,

    pub speaker_mode: i32,
    pub prevent_skip: bool,

    pub touch_mode: i32,
    pub ts_calibration_data: TouchscreenParameter,

    pub pitch_mode_semitone: bool,
    pub pitch_mode_timestretch: bool,

    pub usb_hid: bool,
    pub usb_keypad_mode: i32,

    pub usb_skip_first_drive: bool,

    pub player_name: String,

    pub compressor_settings: CompressorSettings,

    pub sleeptimer_duration: i32,
    pub sleeptimer_on_startup: bool,
    pub keypress_restarts_sleeptimer: bool,

    pub show_shutdown_message: bool,

    pub hotkey_wps: i32,
    pub hotkey_tree: i32,

    pub resume_rewind: i32,

    pub depth_3d: i32,

    pub roll_off: i32,

    pub power_mode: i32,

    pub keyclick_hardware: bool,

    pub start_directory: String,
    pub root_menu_customized: bool,
    pub shortcuts_replaces_qs: bool,

    pub play_frequency: i32,
    pub volume_limit: i32,

    pub volume_adjust_mode: i32,
    pub volume_adjust_norm_steps: i32,

    pub surround_enabled: i32,
    pub surround_balance: i32,
    pub surround_fx1: i32,
    pub surround_fx2: i32,
    pub surround_method2: bool,
    pub surround_mix: i32,

    pub pbe: i32,
    pub pbe_precut: i32,

    pub afr_enabled: i32,

    pub governor: i32,
    pub stereosw_mode: i32,
}

impl From<crate::UserSettings> for UserSettings {
    fn from(settings: crate::UserSettings) -> Self {
        let home = std::env::var("HOME").unwrap();
        Self {
            music_dir: std::env::var("ROCKBOX_LIBRARY")
                .unwrap_or_else(|_| format!("{}/Music", home)),
            balance: settings.balance,
            bass: settings.bass,
            treble: settings.treble,
            channel_config: settings.channel_config,
            stereo_width: settings.stereo_width,
            bass_cutoff: 0,
            treble_cutoff: 0,
            crossfade: settings.crossfade,
            crossfade_fade_in_delay: settings.crossfade_fade_in_delay,
            crossfade_fade_out_delay: settings.crossfade_fade_out_delay,
            crossfade_fade_in_duration: settings.crossfade_fade_in_duration,
            crossfade_fade_out_duration: settings.crossfade_fade_out_duration,
            crossfade_fade_out_mixmode: settings.crossfade_fade_out_mixmode,
            replaygain_settings: ReplaygainSettings::from(settings.replaygain_settings),
            crossfeed: settings.crossfeed,
            crossfeed_direct_gain: settings.crossfeed_direct_gain as i32,
            crossfeed_cross_gain: settings.crossfeed_cross_gain as i32,
            crossfeed_hf_attenuation: settings.crossfeed_hf_attenuation as i32,
            crossfeed_hf_cutoff: settings.crossfeed_hf_cutoff,
            eq_enabled: settings.eq_enabled,
            eq_precut: settings.eq_precut as u32,
            eq_band_settings: settings
                .eq_band_settings
                .into_iter()
                .map(EqBandSetting::from)
                .collect(),
            beep: settings.beep,
            keyclick: settings.keyclick,
            keyclick_repeats: settings.keyclick_repeats,
            dithering_enabled: settings.dithering_enabled,
            timestretch_enabled: settings.timestretch_enabled,
            list_accel_start_delay: settings.list_accel_start_delay,
            list_accel_wait: settings.list_accel_wait,
            touchpad_sensitivity: settings.touchpad_sensitivity,
            touchpad_deadzone: settings.touchpad_deadzone,
            pause_rewind: settings.pause_rewind,
            unplug_mode: settings.unplug_mode,
            unplug_autoresume: settings.unplug_autoresume,
            timeformat: settings.timeformat,
            disk_spindown: settings.disk_spindown,
            buffer_margin: settings.buffer_margin,
            dirfilter: settings.dirfilter,
            show_filename_ext: settings.show_filename_ext,
            default_codepage: settings.default_codepage,
            hold_lr_for_scroll_in_list: settings.hold_lr_for_scroll_in_list,
            play_selected: settings.play_selected,
            single_mode: settings.single_mode,
            party_mode: settings.party_mode,
            cuesheet: settings.cuesheet,
            car_adapter_mode: settings.car_adapter_mode,
            car_adapter_mode_delay: settings.car_adapter_mode_delay,
            start_in_screen: settings.start_in_screen,
            ff_rewind_min_step: settings.ff_rewind_min_step,
            ff_rewind_accel: settings.ff_rewind_accel,
            peak_meter_release: settings.peak_meter_release,
            peak_meter_hold: settings.peak_meter_hold,
            peak_meter_clip_hold: settings.peak_meter_clip_hold,
            peak_meter_dbfs: settings.peak_meter_dbfs,
            peak_meter_min: settings.peak_meter_min,
            peak_meter_max: settings.peak_meter_max,
            wps_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.wps_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            sbs_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.sbs_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            lang_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.lang_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            playlist_catalog_dir: unsafe {
                CStr::from_ptr(cast_ptr!(settings.playlist_catalog_dir.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            skip_length: settings.skip_length,
            max_files_in_dir: settings.max_files_in_dir,
            max_files_in_playlist: settings.max_files_in_playlist,
            volume_type: settings.volume_type,
            battery_display: settings.battery_display,
            show_icons: settings.show_icons,
            statusbar: settings.statusbar,
            scrollbar: settings.scrollbar,
            scrollbar_width: settings.scrollbar_width,
            list_line_padding: settings.list_line_padding,
            list_separator_height: settings.list_separator_height,
            list_separator_color: settings.list_separator_color,
            browse_current: settings.browse_current,
            scroll_paginated: settings.scroll_paginated,
            list_wraparound: settings.list_wraparound,
            list_order: settings.list_order,
            scroll_speed: settings.scroll_speed,
            bidir_limit: settings.bidir_limit,
            scroll_delay: settings.scroll_delay,
            scroll_step: settings.scroll_step,
            autoloadbookmark: settings.autoloadbookmark,
            autocreatebookmark: settings.autocreatebookmark,
            autoupdatebookmark: settings.autoupdatebookmark,
            usemrb: settings.usemrb,
            dircache: settings.dircache,
            tagcache_ram: settings.tagcache_ram,
            tagcache_autoupdate: settings.tagcache_autoupdate,
            autoresume_enable: settings.autoresume_enable,
            autoresume_automatic: settings.autoresume_automatic,
            autoresume_paths: unsafe {
                CStr::from_ptr(cast_ptr!(settings.autoresume_paths.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            runtimedb: settings.runtimedb,
            tagcache_scan_paths: unsafe {
                CStr::from_ptr(cast_ptr!(settings.tagcache_scan_paths.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            tagcache_db_path: unsafe {
                CStr::from_ptr(cast_ptr!(settings.tagcache_db_path.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            backdrop_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.backdrop_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            bg_color: settings.bg_color,
            fg_color: settings.fg_color,
            lss_color: settings.lss_color,
            lse_color: settings.lse_color,
            lst_color: settings.lst_color,
            colors_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.colors_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            browser_default: settings.browser_default,
            repeat_mode: settings.repeat_mode,
            next_folder: settings.next_folder,
            constrain_next_folder: settings.constrain_next_folder,
            recursive_dir_insert: settings.recursive_dir_insert,
            fade_on_stop: settings.fade_on_stop,
            playlist_shuffle: settings.playlist_shuffle,
            warnon_erase_dynplaylist: settings.warnon_erase_dynplaylist,
            keep_current_track_on_replace_playlist: settings.keep_current_track_on_replace_playlist,
            show_shuffled_adding_options: settings.show_shuffled_adding_options,
            show_queue_options: settings.show_queue_options,
            album_art: settings.album_art,
            rewind_across_tracks: settings.rewind_across_tracks,
            playlist_viewer_icons: settings.playlist_viewer_icons,
            playlist_viewer_indices: settings.playlist_viewer_indices,
            playlist_viewer_track_display: settings.playlist_viewer_track_display,
            talk_menu: settings.talk_menu,
            talk_dir: settings.talk_dir,
            talk_dir_clip: settings.talk_dir_clip,
            talk_file: settings.talk_file,
            talk_file_clip: settings.talk_file_clip,
            talk_filetype: settings.talk_filetype,
            talk_battery_level: settings.talk_battery_level,
            talk_mixer_amp: settings.talk_mixer_amp,
            sort_case: settings.sort_case,
            sort_dir: settings.sort_dir,
            sort_file: settings.sort_file,
            interpret_numbers: settings.interpret_numbers,
            poweroff: settings.poweroff,
            battery_capacity: settings.battery_capacity,
            battery_type: settings.battery_type,
            spdif_enable: settings.spdif_enable,
            usb_charging: settings.usb_charging,
            contrast: settings.contrast,
            invert: settings.invert,
            flip_display: settings.flip_display,
            cursor_style: settings.cursor_style,
            screen_scroll_step: settings.screen_scroll_step,
            show_path_in_browser: settings.show_path_in_browser,
            offset_out_of_view: settings.offset_out_of_view,
            disable_mainmenu_scrolling: settings.disable_mainmenu_scrolling,
            icon_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.icon_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            viewers_icon_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.viewers_icon_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            font_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.font_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            glyphs_to_cache: settings.glyphs_to_cache,
            kbd_file: unsafe {
                CStr::from_ptr(cast_ptr!(settings.kbd_file.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            backlight_timeout: settings.backlight_timeout,
            caption_backlight: settings.caption_backlight,
            bl_filter_first_keypress: settings.bl_filter_first_keypress,
            backlight_timeout_plugged: settings.backlight_timeout_plugged,
            bt_selective_softlock_actions: settings.bt_selective_softlock_actions,
            bt_selective_softlock_actions_mask: settings.bt_selective_softlock_actions_mask,
            bl_selective_actions: settings.bl_selective_actions,
            bl_selective_actions_mask: settings.bl_selective_actions_mask,
            backlight_on_button_hold: settings.backlight_on_button_hold,
            lcd_sleep_after_backlight_off: settings.lcd_sleep_after_backlight_off,
            brightness: settings.brightness,
            speaker_mode: settings.speaker_mode,
            prevent_skip: settings.prevent_skip,
            touch_mode: settings.touch_mode,
            ts_calibration_data: TouchscreenParameter::from(settings.ts_calibration_data),
            pitch_mode_semitone: settings.pitch_mode_semitone,
            pitch_mode_timestretch: settings.pitch_mode_timestretch,
            usb_hid: settings.usb_hid,
            usb_keypad_mode: settings.usb_keypad_mode,
            usb_skip_first_drive: settings.usb_skip_first_drive,
            player_name: unsafe {
                CStr::from_ptr(cast_ptr!(settings.player_name.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            compressor_settings: CompressorSettings::from(settings.compressor_settings),
            sleeptimer_duration: settings.sleeptimer_duration,
            sleeptimer_on_startup: settings.sleeptimer_on_startup,
            keypress_restarts_sleeptimer: settings.keypress_restarts_sleeptimer,
            show_shutdown_message: settings.show_shutdown_message,
            hotkey_wps: settings.hotkey_wps,
            hotkey_tree: settings.hotkey_tree,
            resume_rewind: settings.resume_rewind,
            depth_3d: settings.depth_3d,
            roll_off: settings.roll_off,
            power_mode: settings.power_mode,
            keyclick_hardware: settings.keyclick_hardware,
            start_directory: unsafe {
                CStr::from_ptr(cast_ptr!(settings.start_directory.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            },
            root_menu_customized: settings.root_menu_customized,
            shortcuts_replaces_qs: settings.shortcuts_replaces_qs,
            play_frequency: settings.play_frequency,
            volume_limit: settings.volume_limit,
            volume_adjust_mode: settings.volume_adjust_mode,
            volume_adjust_norm_steps: settings.volume_adjust_norm_steps,
            surround_enabled: settings.surround_enabled,
            surround_balance: settings.surround_balance,
            surround_fx1: settings.surround_fx1,
            surround_fx2: settings.surround_fx2,
            surround_method2: settings.surround_method2,
            surround_mix: settings.surround_mix,
            pbe: settings.pbe,
            pbe_precut: settings.pbe_precut,
            afr_enabled: settings.afr_enabled,
            governor: settings.governor,
            stereosw_mode: settings.stereosw_mode,
        }
    }
}

/// One entry in the `airplay_receivers` list in settings.toml.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AirPlayReceiverConfig {
    pub host: String,
    /// RAOP port (default: 5000)
    pub port: Option<u16>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct NewGlobalSettings {
    pub music_dir: Option<String>,
    pub playlist_shuffle: Option<bool>,
    pub repeat_mode: Option<i32>,
    pub bass: Option<i32>,
    pub treble: Option<i32>,
    pub bass_cutoff: Option<i32>,
    pub treble_cutoff: Option<i32>,
    pub crossfade: Option<i32>,
    pub fade_on_stop: Option<bool>,
    pub fade_in_delay: Option<i32>,
    pub fade_in_duration: Option<i32>,
    pub fade_out_delay: Option<i32>,
    pub fade_out_duration: Option<i32>,
    pub fade_out_mixmode: Option<i32>,
    pub balance: Option<i32>,
    pub stereo_width: Option<i32>,
    pub stereosw_mode: Option<i32>,
    pub surround_enabled: Option<i32>,
    pub surround_balance: Option<i32>,
    pub surround_fx1: Option<i32>,
    pub surround_fx2: Option<i32>,
    pub party_mode: Option<bool>,
    pub channel_config: Option<i32>,
    pub player_name: Option<String>,
    pub eq_enabled: Option<bool>,
    pub eq_band_settings: Option<Vec<EqBandSetting>>,
    pub replaygain_settings: Option<ReplaygainSettings>,
    pub compressor_settings: Option<CompressorSettings>,
    pub eq_precut: Option<i32>,
    pub crossfeed: Option<i32>,
    pub crossfeed_direct_gain: Option<i32>,
    pub crossfeed_cross_gain: Option<i32>,
    pub crossfeed_hf_attenuation: Option<i32>,
    pub crossfeed_hf_cutoff: Option<i32>,
    pub dithering_enabled: Option<bool>,
    pub surround_mix: Option<i32>,
    pub surround_method2: Option<bool>,
    pub afr_enabled: Option<i32>,
    pub pbe: Option<i32>,
    pub pbe_precut: Option<i32>,
    /// Audio output sink: "builtin" (default), "fifo", "airplay", or "squeezelite"
    pub audio_output: Option<String>,
    /// Path for the FIFO sink, e.g. "/tmp/rockbox.fifo" or "-" for stdout
    pub fifo_path: Option<String>,
    /// Single AirPlay (RAOP) receiver — kept for backward compatibility.
    /// Prefer `airplay_receivers` for multi-room setups.
    pub airplay_host: Option<String>,
    /// RAOP port for the single receiver (default: 5000)
    pub airplay_port: Option<u16>,
    /// Multi-room AirPlay receiver list. Takes precedence over `airplay_host`/`airplay_port`.
    pub airplay_receivers: Option<Vec<AirPlayReceiverConfig>>,
    /// Slim Protocol control port for the squeezelite sink (default: 3483)
    pub squeezelite_port: Option<u16>,
    /// HTTP audio stream port for the squeezelite sink (default: 9999)
    pub squeezelite_http_port: Option<u16>,
    /// Enable the UPnP/DLNA ContentDirectory media server (default: false)
    pub upnp_server_enabled: Option<bool>,
    /// HTTP port for the UPnP media server (default: 7878)
    pub upnp_server_port: Option<u16>,
    /// Friendly device name advertised via SSDP (default: "Rockbox")
    pub upnp_friendly_name: Option<String>,
    /// HTTP port for the UPnP PCM WAV stream sink (default: 7879)
    pub upnp_http_port: Option<u16>,
    /// AVTransport control URL of a UPnP renderer to auto-command when the
    /// upnp sink starts (e.g. "http://192.168.1.x:PORT/AVTransport/control")
    pub upnp_renderer_url: Option<String>,
    /// Enable the UPnP/DLNA MediaRenderer:1 (default: false)
    pub upnp_renderer_enabled: Option<bool>,
    /// HTTP port for the UPnP renderer (default: 7880)
    pub upnp_renderer_port: Option<u16>,
    /// IP address of the Chromecast device (required for chromecast output)
    pub chromecast_host: Option<String>,
    /// Cast protocol port on the Chromecast device (default: 8009)
    pub chromecast_port: Option<u16>,
    /// HTTP port for the Chromecast WAV stream (default: 7881)
    pub chromecast_http_port: Option<u16>,
    /// Host address of the Snapcast TCP source (required for snapcast_tcp output)
    pub snapcast_tcp_host: Option<String>,
    /// TCP port for the Snapcast source (default: 4953)
    pub snapcast_tcp_port: Option<u16>,
    /// Subsonic/Navidrome-compatible API server username (for connecting clients).
    pub subsonic_username: Option<String>,
    /// Subsonic/Navidrome-compatible API server password.
    pub subsonic_password: Option<String>,
    /// Port for the Subsonic-compatible API server (default: 4533).
    pub subsonic_port: Option<u16>,
    /// Port for the Jellyfin-compatible API server (default: 8096). The
    /// server reuses `subsonic_username` / `subsonic_password` as the
    /// Jellyfin credentials, so it activates whenever those are set.
    pub jellyfin_port: Option<u16>,
    /// HTTP port for the CMAF sink (HLS + DASH manifests + fMP4 segments). Default: 7882.
    pub cmaf_http_port: Option<u16>,
    /// AAC-LC bitrate for the CMAF sink, in bits/sec. Default: 128000.
    pub cmaf_bitrate: Option<u32>,
    /// Optional directory the CMAF sink mirrors `init.mp4`, `seg/N.m4s`,
    /// and the HLS/DASH manifests into. When absent, segments stay in
    /// memory only. Useful for serving the same artefacts from an external
    /// HTTP server (nginx, Caddy, a CDN origin).
    pub cmaf_segment_dir: Option<String>,
    /// Enable the S3-compatible HTTP API (default: false). Requires
    /// `s3_access_key` and `s3_secret_key` to be set.
    pub s3_enabled: Option<bool>,
    /// Listen address for the S3 server (default: "0.0.0.0").
    pub s3_host: Option<String>,
    /// Port for the S3 server (default: 9000).
    pub s3_port: Option<u16>,
    /// AWS access key ID for SigV4 auth.
    pub s3_access_key: Option<String>,
    /// AWS secret access key for SigV4 auth.
    pub s3_secret_key: Option<String>,
    /// Last.fm API key. When set, the Jellyfin /Items/{id}/Similar
    /// endpoints (and legacy `/Artists`/`/Albums` variants) call Last.fm
    /// to expand seed items into a similar-item list. Absent → the
    /// Similar endpoints return an empty result.
    pub lastfm_api_key: Option<String>,
    /// MusicBrainz `User-Agent` string, e.g.
    /// `"rockbox-zig/0.1 ( contact@example.com )"`. When set, MBIDs
    /// returned by Last.fm are cross-referenced against MusicBrainz to
    /// canonicalize artist/album names before matching them back to
    /// the local library. Absent → we fall back to Last.fm's name-only
    /// output.
    pub musicbrainz_user_agent: Option<String>,
    /// Compute a Chromaprint acoustic fingerprint for every local track
    /// (default: true). Fingerprints are stored in the library database and
    /// are what makes two copies of the same recording — a FLAC and its MP3 —
    /// recognisable as duplicates. Set to `false` to skip the pass entirely.
    pub fingerprint_enabled: Option<bool>,
    /// How many files the fingerprint pass decodes at once. Defaults to half
    /// the machine's cores, capped at 4 — enough to use an idle machine,
    /// few enough that playback never competes with it for disk or CPU.
    pub fingerprint_concurrency: Option<u32>,
    /// AcoustID API key. When set, fingerprints are looked up against
    /// <https://acoustid.org> and the MusicBrainz recordings they resolve to
    /// are used to fill in tags a file never had — what MusicBrainz Picard
    /// does. Absent → fingerprints are still computed, but nothing is looked
    /// up. Keys are free: <https://acoustid.org/new-application>. The
    /// `ACOUSTID_API_KEY` environment variable overrides this.
    pub acoustid_api_key: Option<String>,
    /// Look fingerprints up against AcoustID (default: true when a key is
    /// set). Set to `false` to keep fingerprinting locally while making no
    /// network requests.
    pub acoustid_enabled: Option<bool>,
    /// Also write identified metadata into the audio files themselves, the way
    /// Picard does (default: false). Only fields the file left empty are ever
    /// written — an existing tag is never overwritten — but this does modify
    /// the user's files, so it is opt-in. Off, identified metadata goes to the
    /// library database only.
    pub acoustid_write_tags: Option<bool>,
}

impl From<UserSettings> for NewGlobalSettings {
    fn from(settings: UserSettings) -> Self {
        Self {
            music_dir: None,
            playlist_shuffle: Some(settings.playlist_shuffle),
            repeat_mode: Some(settings.repeat_mode),
            bass: Some(settings.bass),
            treble: Some(settings.treble),
            bass_cutoff: Some(settings.bass_cutoff),
            treble_cutoff: Some(settings.treble_cutoff),
            crossfade: Some(settings.crossfade),
            fade_on_stop: Some(settings.fade_on_stop),
            fade_in_delay: Some(settings.crossfade_fade_in_delay),
            fade_in_duration: Some(settings.crossfade_fade_in_duration),
            fade_out_delay: Some(settings.crossfade_fade_out_delay),
            fade_out_duration: Some(settings.crossfade_fade_out_duration),
            fade_out_mixmode: Some(settings.crossfade_fade_out_mixmode),
            balance: Some(settings.balance),
            stereo_width: Some(settings.stereo_width),
            stereosw_mode: Some(settings.stereosw_mode),
            surround_enabled: Some(settings.surround_enabled),
            surround_balance: Some(settings.surround_balance),
            surround_fx1: Some(settings.surround_fx1),
            surround_fx2: Some(settings.surround_fx2),
            party_mode: Some(settings.party_mode),
            channel_config: Some(settings.channel_config),
            player_name: Some(settings.player_name),
            eq_enabled: Some(settings.eq_enabled),
            eq_band_settings: Some(settings.eq_band_settings),
            replaygain_settings: Some(settings.replaygain_settings),
            compressor_settings: Some(settings.compressor_settings),
            eq_precut: Some(settings.eq_precut as i32),
            crossfeed: Some(settings.crossfeed),
            crossfeed_direct_gain: Some(settings.crossfeed_direct_gain as i32),
            crossfeed_cross_gain: Some(settings.crossfeed_cross_gain as i32),
            crossfeed_hf_attenuation: Some(settings.crossfeed_hf_attenuation as i32),
            crossfeed_hf_cutoff: Some(settings.crossfeed_hf_cutoff as i32),
            dithering_enabled: Some(settings.dithering_enabled),
            surround_mix: Some(settings.surround_mix),
            surround_method2: Some(settings.surround_method2),
            afr_enabled: Some(settings.afr_enabled),
            pbe: Some(settings.pbe),
            pbe_precut: Some(settings.pbe_precut),
            audio_output: None,
            fifo_path: None,
            airplay_host: None,
            airplay_port: None,
            airplay_receivers: None,
            squeezelite_port: None,
            squeezelite_http_port: None,
            upnp_server_enabled: None,
            upnp_server_port: None,
            upnp_friendly_name: None,
            upnp_http_port: None,
            upnp_renderer_url: None,
            upnp_renderer_enabled: None,
            upnp_renderer_port: None,
            chromecast_host: None,
            chromecast_port: None,
            chromecast_http_port: None,
            snapcast_tcp_host: None,
            snapcast_tcp_port: None,
            subsonic_username: None,
            subsonic_password: None,
            subsonic_port: None,
            jellyfin_port: None,
            cmaf_http_port: None,
            cmaf_bitrate: None,
            cmaf_segment_dir: None,
            s3_enabled: None,
            s3_host: None,
            s3_port: None,
            s3_access_key: None,
            s3_secret_key: None,
            lastfm_api_key: None,
            musicbrainz_user_agent: None,
            fingerprint_enabled: None,
            fingerprint_concurrency: None,
            acoustid_api_key: None,
            acoustid_enabled: None,
            acoustid_write_tags: None,
        }
    }
}
