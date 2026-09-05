//! MPEG-DASH MPD (Media Presentation Description) parsing and segment-URL
//! planning.
//!
//! [`parse_mpd`] reads the manifest XML into a small model; [`build_plan`]
//! then picks the best audio representation and expands its addressing
//! (SegmentTemplate with `$Number$`/`$Time$`, SegmentTimeline, SegmentList,
//! or a single-file BaseURL) into concrete segment URLs for the fetch loop.
//!
//! Supported:
//! - static (VOD) presentations, all periods in order
//! - dynamic (live) presentations via SegmentTimeline refresh, or an
//!   open-ended `$Number$` template paced by the segment duration
//!
//! Not supported: ContentProtection (DRM), xlink, multiple BaseURLs per
//! level (the first wins).

use std::io;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::url_join;

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Mpd {
    /// `type="dynamic"` — a live presentation that updates over time.
    pub dynamic: bool,
    /// `mediaPresentationDuration` in seconds.
    pub media_presentation_duration: Option<f64>,
    /// `minimumUpdatePeriod` in seconds (dynamic refresh pacing).
    pub minimum_update_period: Option<f64>,
    /// `availabilityStartTime` as Unix seconds (dynamic timing anchor).
    pub availability_start_time: Option<f64>,
    pub base_url: Option<String>,
    pub periods: Vec<Period>,
}

#[derive(Debug, Clone, Default)]
pub struct Period {
    pub start: Option<f64>,
    pub duration: Option<f64>,
    pub base_url: Option<String>,
    pub adaptations: Vec<AdaptationSet>,
}

#[derive(Debug, Clone, Default)]
pub struct AdaptationSet {
    pub content_type: Option<String>,
    pub mime_type: Option<String>,
    pub base_url: Option<String>,
    pub template: Option<SegmentTemplate>,
    pub representations: Vec<Representation>,
}

#[derive(Debug, Clone, Default)]
pub struct Representation {
    pub id: String,
    pub bandwidth: u64,
    pub mime_type: Option<String>,
    pub codecs: Option<String>,
    pub audio_sampling_rate: Option<u32>,
    pub base_url: Option<String>,
    pub template: Option<SegmentTemplate>,
    pub segment_list: Option<SegmentList>,
}

#[derive(Debug, Clone, Default)]
pub struct SegmentTemplate {
    pub initialization: Option<String>,
    pub media: Option<String>,
    pub duration: Option<u64>,
    pub timescale: Option<u64>,
    pub start_number: Option<u64>,
    pub timeline: Option<Vec<TimelineS>>,
}

/// One `<S>` element of a SegmentTimeline.
#[derive(Debug, Clone, Copy)]
pub struct TimelineS {
    pub t: Option<u64>,
    pub d: u64,
    pub r: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SegmentList {
    pub initialization: Option<String>,
    pub media: Vec<String>,
}

// ---------------------------------------------------------------------------
// XML parsing
// ---------------------------------------------------------------------------

/// Parse `xsd:duration` ("PT1H2M3.5S", "P1DT2H") to seconds.
pub fn parse_iso_duration(s: &str) -> Option<f64> {
    let s = s.trim().strip_prefix('P')?;
    let (date_part, time_part) = match s.split_once('T') {
        Some((d, t)) => (d, t),
        None => (s, ""),
    };
    let mut secs = 0.0f64;
    let mut num = String::new();
    for c in date_part.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            secs += match c {
                'Y' => v * 365.0 * 86400.0,
                'M' => v * 30.0 * 86400.0,
                'W' => v * 7.0 * 86400.0,
                'D' => v * 86400.0,
                _ => return None,
            };
        }
    }
    for c in time_part.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            secs += match c {
                'H' => v * 3600.0,
                'M' => v * 60.0,
                'S' => v,
                _ => return None,
            };
        }
    }
    num.is_empty().then_some(secs)
}

/// Parse `xsd:dateTime` ("2024-05-01T12:00:00Z", optional fraction/offset)
/// to Unix seconds.
pub fn parse_iso_datetime(s: &str) -> Option<f64> {
    let s = s.trim();
    let (date, rest) = s.split_once('T')?;
    let mut d = date.splitn(3, '-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;

    // Split the time from a trailing zone: 'Z', '+hh:mm' or '-hh:mm'.
    let (time, offset_secs) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0i64)
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        let (t, z) = rest.split_at(pos);
        let sign = if z.starts_with('-') { -1i64 } else { 1 };
        let z = &z[1..];
        let (zh, zm) = z.split_once(':').unwrap_or((z, "0"));
        let off = sign * (zh.parse::<i64>().ok()? * 3600 + zm.parse::<i64>().ok()? * 60);
        (t, off)
    } else {
        (rest, 0)
    };
    let mut t = time.splitn(3, ':');
    let hour: i64 = t.next()?.parse().ok()?;
    let minute: i64 = t.next()?.parse().ok()?;
    let sec: f64 = t.next().unwrap_or("0").parse().ok()?;

    // Days since Unix epoch (Howard Hinnant's civil-days algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some((days * 86400 + hour * 3600 + minute * 60 - offset_secs) as f64 + sec)
}

/// Parse an MPD document.
pub fn parse_mpd(xml: &str) -> io::Result<Mpd> {
    let mut reader = Reader::from_str(xml);
    let mut mpd = Mpd::default();
    let mut saw_mpd = false;

    let mut period: Option<Period> = None;
    let mut adapt: Option<AdaptationSet> = None;
    let mut rep: Option<Representation> = None;
    let mut seg_list: Option<SegmentList> = None;
    // (template, belongs_to_representation)
    let mut template: Option<(SegmentTemplate, bool)> = None;
    let mut in_timeline = false;
    let mut in_base_url = false;

    loop {
        let ev = reader
            .read_event()
            .map_err(|e| err(format!("MPD XML parse error: {e}")))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = e.local_name();
                let name = name.as_ref();
                let get = |key: &str| -> Option<String> {
                    e.attributes().flatten().find_map(|a| {
                        (a.key.local_name().as_ref() == key.as_bytes())
                            .then(|| String::from_utf8_lossy(&a.value).into_owned())
                    })
                };
                match name {
                    b"MPD" => {
                        saw_mpd = true;
                        mpd.dynamic = get("type").as_deref() == Some("dynamic");
                        mpd.media_presentation_duration = get("mediaPresentationDuration")
                            .as_deref()
                            .and_then(parse_iso_duration);
                        mpd.minimum_update_period = get("minimumUpdatePeriod")
                            .as_deref()
                            .and_then(parse_iso_duration);
                        mpd.availability_start_time = get("availabilityStartTime")
                            .as_deref()
                            .and_then(parse_iso_datetime);
                    }
                    b"Period" => {
                        let p = Period {
                            start: get("start").as_deref().and_then(parse_iso_duration),
                            duration: get("duration").as_deref().and_then(parse_iso_duration),
                            ..Default::default()
                        };
                        if empty {
                            mpd.periods.push(p);
                        } else {
                            period = Some(p);
                        }
                    }
                    b"AdaptationSet" if period.is_some() => {
                        let a = AdaptationSet {
                            content_type: get("contentType"),
                            mime_type: get("mimeType"),
                            ..Default::default()
                        };
                        if empty {
                            period.as_mut().unwrap().adaptations.push(a);
                        } else {
                            adapt = Some(a);
                        }
                    }
                    b"Representation" if adapt.is_some() => {
                        let r = Representation {
                            id: get("id").unwrap_or_default(),
                            bandwidth: get("bandwidth").and_then(|v| v.parse().ok()).unwrap_or(0),
                            mime_type: get("mimeType"),
                            codecs: get("codecs"),
                            audio_sampling_rate: get("audioSamplingRate")
                                .and_then(|v| v.split('/').next()?.trim().parse().ok()),
                            ..Default::default()
                        };
                        if empty {
                            adapt.as_mut().unwrap().representations.push(r);
                        } else {
                            rep = Some(r);
                        }
                    }
                    b"SegmentTemplate" => {
                        let t = SegmentTemplate {
                            initialization: get("initialization"),
                            media: get("media"),
                            duration: get("duration").and_then(|v| v.parse().ok()),
                            timescale: get("timescale").and_then(|v| v.parse().ok()),
                            start_number: get("startNumber").and_then(|v| v.parse().ok()),
                            timeline: None,
                        };
                        let for_rep = rep.is_some();
                        if empty {
                            match (&mut rep, &mut adapt) {
                                (Some(r), _) => r.template = Some(t),
                                (None, Some(a)) => a.template = Some(t),
                                _ => {}
                            }
                        } else {
                            template = Some((t, for_rep));
                        }
                    }
                    b"SegmentTimeline" => {
                        if let Some((t, _)) = template.as_mut() {
                            t.timeline = Some(Vec::new());
                            in_timeline = true;
                        }
                    }
                    b"S" if in_timeline => {
                        if let Some((t, _)) = template.as_mut() {
                            if let Some(tl) = t.timeline.as_mut() {
                                tl.push(TimelineS {
                                    t: get("t").and_then(|v| v.parse().ok()),
                                    d: get("d").and_then(|v| v.parse().ok()).unwrap_or(0),
                                    r: get("r").and_then(|v| v.parse().ok()).unwrap_or(0),
                                });
                            }
                        }
                    }
                    b"SegmentList" if rep.is_some() => {
                        let l = SegmentList::default();
                        if empty {
                            rep.as_mut().unwrap().segment_list = Some(l);
                        } else {
                            seg_list = Some(l);
                        }
                    }
                    b"Initialization" => {
                        let src = get("sourceURL");
                        if let Some(l) = seg_list.as_mut() {
                            l.initialization = src;
                        }
                    }
                    b"SegmentURL" => {
                        if let (Some(l), Some(m)) = (seg_list.as_mut(), get("media")) {
                            l.media.push(m);
                        }
                    }
                    b"BaseURL" if !empty => in_base_url = true,
                    _ => {}
                }
            }
            Event::Text(t) => {
                if in_base_url {
                    let text = t
                        .unescape()
                        .map(|c| c.trim().to_string())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        let slot = if let Some(r) = rep.as_mut() {
                            &mut r.base_url
                        } else if let Some(a) = adapt.as_mut() {
                            &mut a.base_url
                        } else if let Some(p) = period.as_mut() {
                            &mut p.base_url
                        } else {
                            &mut mpd.base_url
                        };
                        if slot.is_none() {
                            *slot = Some(text);
                        }
                    }
                }
            }
            Event::End(ref e) => match e.local_name().as_ref() {
                b"BaseURL" => in_base_url = false,
                b"SegmentTimeline" => in_timeline = false,
                b"SegmentTemplate" => {
                    if let Some((t, for_rep)) = template.take() {
                        match (for_rep, &mut rep, &mut adapt) {
                            (true, Some(r), _) => r.template = Some(t),
                            (false, _, Some(a)) => a.template = Some(t),
                            _ => {}
                        }
                    }
                }
                b"SegmentList" => {
                    if let (Some(l), Some(r)) = (seg_list.take(), rep.as_mut()) {
                        r.segment_list = Some(l);
                    }
                }
                b"Representation" => {
                    if let (Some(r), Some(a)) = (rep.take(), adapt.as_mut()) {
                        a.representations.push(r);
                    }
                }
                b"AdaptationSet" => {
                    if let (Some(a), Some(p)) = (adapt.take(), period.as_mut()) {
                        p.adaptations.push(a);
                    }
                }
                b"Period" => {
                    if let Some(p) = period.take() {
                        mpd.periods.push(p);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_mpd {
        return Err(err("document has no MPD root element"));
    }
    Ok(mpd)
}

// ---------------------------------------------------------------------------
// Segment planning
// ---------------------------------------------------------------------------

/// One concrete media segment to fetch.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRef {
    pub url: String,
    /// Monotonic key for live-refresh dedup: the timeline `t` (or the
    /// template number). A refresh only appends segments with a larger key.
    pub key: u64,
    /// Init segment in effect for this segment, if any.
    pub init_url: Option<String>,
}

/// Open-ended `$Number$` continuation for dynamic MPDs with a plain
/// duration-paced template (no timeline).
#[derive(Debug, Clone)]
pub struct OpenTemplate {
    media_template: String,
    base: String,
    rep_id: String,
    bandwidth: u64,
    pub next_number: u64,
    /// Nominal seconds per segment (fetch pacing / 404 backoff).
    pub segment_seconds: f64,
    pub init_url: Option<String>,
}

impl OpenTemplate {
    /// The URL for segment `number`.
    pub fn media_url(&self, number: u64) -> String {
        let rel = fill_template(
            &self.media_template,
            &self.rep_id,
            self.bandwidth,
            number,
            0,
        );
        url_join(&self.base, &rel)
    }
}

/// The complete fetch plan for the chosen audio representation.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Known, finite segments (all of them for static; the current window
    /// for a dynamic timeline).
    pub segments: Vec<SegmentRef>,
    /// Present for dynamic duration-paced templates: generate numbers
    /// forever instead of using `segments`.
    pub open_template: Option<OpenTemplate>,
    /// The representation is one plain media file — hand it to the regular
    /// progressive-download path instead.
    pub single_file: Option<String>,
    pub total_duration: Option<f64>,
    pub dynamic: bool,
    /// Suggested manifest re-fetch interval for dynamic timelines (seconds).
    pub update_period: f64,
    pub sample_rate: Option<u32>,
    pub codecs: Option<String>,
}

/// Substitute `$RepresentationID$`, `$Bandwidth$`, `$Number$`, `$Time$`
/// (with optional `%0Nd` width) and `$$` in a template string.
fn fill_template(template: &str, rep_id: &str, bandwidth: u64, number: u64, time: u64) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('$') else {
            out.push('$');
            break;
        };
        let ident = &rest[..end];
        rest = &rest[end + 1..];
        if ident.is_empty() {
            out.push('$'); // "$$" escape
            continue;
        }
        let (name, fmt) = match ident.split_once('%') {
            Some((n, f)) => (n, Some(f)),
            None => (ident, None),
        };
        let value: Option<String> = match name {
            "RepresentationID" => Some(rep_id.to_string()),
            "Bandwidth" => Some(bandwidth.to_string()),
            "Number" => Some(number.to_string()),
            "Time" => Some(time.to_string()),
            _ => None,
        };
        match value {
            Some(v) => {
                // "%05d"-style zero padding.
                let width = fmt
                    .and_then(|f| f.strip_prefix('0'))
                    .and_then(|f| f.strip_suffix('d'))
                    .and_then(|w| w.parse::<usize>().ok())
                    .unwrap_or(0);
                if v.len() < width {
                    out.extend(std::iter::repeat_n('0', width - v.len()));
                }
                out.push_str(&v);
            }
            None => {
                // Unknown identifier — reproduce as-is.
                out.push('$');
                out.push_str(ident);
                out.push('$');
            }
        }
    }
    out.push_str(rest);
    out
}

/// Is this adaptation set audio?
fn is_audio(a: &AdaptationSet) -> bool {
    if let Some(ct) = &a.content_type {
        return ct.eq_ignore_ascii_case("audio");
    }
    if let Some(mt) = &a.mime_type {
        return mt.to_ascii_lowercase().starts_with("audio/");
    }
    a.representations.iter().any(|r| {
        r.mime_type
            .as_deref()
            .is_some_and(|m| m.to_ascii_lowercase().starts_with("audio/"))
    })
}

/// Expand a SegmentTimeline into `(t, d)` pairs. `r = -1` repeats until the
/// period end when known, else a bounded window.
fn expand_timeline(
    tl: &[TimelineS],
    timescale: u64,
    period_duration: Option<f64>,
) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut t = 0u64;
    for s in tl {
        if let Some(explicit) = s.t {
            t = explicit;
        }
        if s.d == 0 {
            continue;
        }
        let repeats: u64 = if s.r >= 0 {
            s.r as u64
        } else {
            match period_duration {
                Some(dur) => {
                    let end = (dur * timescale as f64) as u64;
                    (end.saturating_sub(t) / s.d).saturating_sub(1)
                }
                None => 99, // bounded live window; the next refresh extends it
            }
        };
        for _ in 0..=repeats {
            out.push((t, s.d));
            t += s.d;
        }
    }
    out
}

/// Build the fetch plan from a parsed MPD. `mpd_url` anchors relative URLs;
/// `now_unix` is the current wall-clock (Unix seconds), used only for
/// dynamic duration-paced templates.
pub fn build_plan(mpd: &Mpd, mpd_url: &str, now_unix: f64) -> io::Result<Plan> {
    let mpd_base = match &mpd.base_url {
        Some(b) => url_join(mpd_url, b),
        None => mpd_url.to_string(),
    };

    let mut plan = Plan {
        segments: Vec::new(),
        open_template: None,
        single_file: None,
        total_duration: mpd.media_presentation_duration,
        dynamic: mpd.dynamic,
        update_period: mpd.minimum_update_period.unwrap_or(0.0).max(1.0),
        sample_rate: None,
        codecs: None,
    };

    // For dynamic presentations only the newest period matters; a static one
    // plays all periods in order.
    let periods: Vec<&Period> = if mpd.dynamic {
        mpd.periods.last().into_iter().collect()
    } else {
        mpd.periods.iter().collect()
    };
    if periods.is_empty() {
        return Err(err("MPD has no Period"));
    }

    let mut total_from_periods = 0.0f64;
    let mut have_period_durations = true;

    for period in &periods {
        let Some((adapt, rep)) = period
            .adaptations
            .iter()
            .filter(|a| is_audio(a))
            .flat_map(|a| a.representations.iter().map(move |r| (a, r)))
            .max_by_key(|(_, r)| r.bandwidth)
        else {
            continue;
        };
        plan.sample_rate = plan.sample_rate.or(rep.audio_sampling_rate);
        plan.codecs = plan.codecs.clone().or_else(|| rep.codecs.clone());

        // Resolve the BaseURL chain down to this representation.
        let period_base = match &period.base_url {
            Some(b) => url_join(&mpd_base, b),
            None => mpd_base.clone(),
        };
        let adapt_base = match &adapt.base_url {
            Some(b) => url_join(&period_base, b),
            None => period_base,
        };
        let base = match &rep.base_url {
            Some(b) => url_join(&adapt_base, b),
            None => adapt_base,
        };

        match period.duration.or_else(|| {
            (periods.len() == 1)
                .then_some(mpd.media_presentation_duration)
                .flatten()
        }) {
            Some(d) => total_from_periods += d,
            None => have_period_durations = false,
        }

        // Representation template overrides the adaptation-level one;
        // attributes are merged (representation wins per attribute).
        let template = merge_templates(rep.template.as_ref(), adapt.template.as_ref());

        if let Some(t) = template {
            let timescale = t.timescale.unwrap_or(1).max(1);
            let start_number = t.start_number.unwrap_or(1);
            let media = t
                .media
                .clone()
                .ok_or_else(|| err("SegmentTemplate has no media attribute"))?;
            let init_url = t.initialization.as_ref().map(|i| {
                url_join(
                    &base,
                    &fill_template(i, &rep.id, rep.bandwidth, start_number, 0),
                )
            });

            if let Some(tl) = &t.timeline {
                let period_dur = period.duration.or(if periods.len() == 1 {
                    mpd.media_presentation_duration
                } else {
                    None
                });
                let expanded = expand_timeline(tl, timescale, period_dur);
                for (i, (time, _d)) in expanded.iter().enumerate() {
                    let number = start_number + i as u64;
                    let rel = fill_template(&media, &rep.id, rep.bandwidth, number, *time);
                    plan.segments.push(SegmentRef {
                        url: url_join(&base, &rel),
                        key: *time,
                        init_url: init_url.clone(),
                    });
                }
            } else {
                let seg_dur = t.duration.unwrap_or(0);
                if seg_dur == 0 {
                    return Err(err("SegmentTemplate has neither timeline nor duration"));
                }
                let seg_seconds = seg_dur as f64 / timescale as f64;
                if mpd.dynamic {
                    // Live edge: how many whole segments have elapsed since
                    // availabilityStartTime (+ period start); step back a few
                    // for a safety buffer.
                    let elapsed = mpd
                        .availability_start_time
                        .map(|ast| (now_unix - ast - period.start.unwrap_or(0.0)).max(0.0))
                        .unwrap_or(0.0);
                    let live_edge = start_number + (elapsed / seg_seconds) as u64;
                    let next = live_edge.saturating_sub(3).max(start_number);
                    plan.open_template = Some(OpenTemplate {
                        media_template: media,
                        base,
                        rep_id: rep.id.clone(),
                        bandwidth: rep.bandwidth,
                        next_number: next,
                        segment_seconds: seg_seconds,
                        init_url,
                    });
                    return Ok(plan);
                }
                let period_dur = period
                    .duration
                    .or(mpd.media_presentation_duration)
                    .ok_or_else(|| err("static MPD has no period/presentation duration"))?;
                let count = (period_dur / seg_seconds).ceil() as u64;
                for i in 0..count {
                    let number = start_number + i;
                    let rel = fill_template(&media, &rep.id, rep.bandwidth, number, 0);
                    plan.segments.push(SegmentRef {
                        url: url_join(&base, &rel),
                        key: number,
                        init_url: init_url.clone(),
                    });
                }
            }
        } else if let Some(list) = &rep.segment_list {
            let init_url = list.initialization.as_ref().map(|i| url_join(&base, i));
            for (i, m) in list.media.iter().enumerate() {
                plan.segments.push(SegmentRef {
                    url: url_join(&base, m),
                    key: i as u64,
                    init_url: init_url.clone(),
                });
            }
        } else {
            // No segmenting at all — the BaseURL is the whole media file.
            if periods.len() == 1 && plan.segments.is_empty() {
                plan.single_file = Some(base);
                return Ok(plan);
            }
            plan.segments.push(SegmentRef {
                url: base,
                key: plan.segments.len() as u64,
                init_url: None,
            });
        }
    }

    if plan.segments.is_empty() && plan.open_template.is_none() {
        return Err(err("MPD has no audio representation with segments"));
    }
    if plan.total_duration.is_none() && have_period_durations && total_from_periods > 0.0 {
        plan.total_duration = Some(total_from_periods);
    }
    Ok(plan)
}

/// Merge representation-level and adaptation-level SegmentTemplates
/// (representation attributes win).
fn merge_templates(
    rep: Option<&SegmentTemplate>,
    adapt: Option<&SegmentTemplate>,
) -> Option<SegmentTemplate> {
    match (rep, adapt) {
        (None, None) => None,
        (Some(r), None) => Some(r.clone()),
        (None, Some(a)) => Some(a.clone()),
        (Some(r), Some(a)) => Some(SegmentTemplate {
            initialization: r
                .initialization
                .clone()
                .or_else(|| a.initialization.clone()),
            media: r.media.clone().or_else(|| a.media.clone()),
            duration: r.duration.or(a.duration),
            timescale: r.timescale.or(a.timescale),
            start_number: r.start_number.or(a.start_number),
            timeline: r.timeline.clone().or_else(|| a.timeline.clone()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://cdn.example.com/dash/stream.mpd";

    #[test]
    fn iso_duration_parsing() {
        assert_eq!(parse_iso_duration("PT10S"), Some(10.0));
        assert_eq!(parse_iso_duration("PT1H2M3.5S"), Some(3723.5));
        assert_eq!(parse_iso_duration("P1DT1S"), Some(86401.0));
        assert_eq!(parse_iso_duration("PT0.5S"), Some(0.5));
        assert_eq!(parse_iso_duration("nope"), None);
    }

    #[test]
    fn iso_datetime_parsing() {
        assert_eq!(parse_iso_datetime("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_iso_datetime("1970-01-02T00:00:00Z"), Some(86400.0));
        // 2024-05-01T12:00:00Z
        assert_eq!(
            parse_iso_datetime("2024-05-01T12:00:00Z"),
            Some(1714564800.0)
        );
        // Same instant expressed with a +02:00 offset.
        assert_eq!(
            parse_iso_datetime("2024-05-01T14:00:00+02:00"),
            Some(1714564800.0)
        );
    }

    #[test]
    fn template_substitution() {
        assert_eq!(
            fill_template("seg-$RepresentationID$-$Number%05d$.m4s", "audio", 0, 42, 0),
            "seg-audio-00042.m4s"
        );
        assert_eq!(
            fill_template("$Time$-$Bandwidth$$$", "r", 128000, 0, 900900),
            "900900-128000$"
        );
    }

    #[test]
    fn static_number_template_plan() {
        let xml = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT30S">
  <Period>
    <AdaptationSet contentType="audio" mimeType="audio/mp4">
      <SegmentTemplate initialization="$RepresentationID$/init.mp4"
                       media="$RepresentationID$/seg-$Number$.m4s"
                       duration="10" timescale="1" startNumber="1"/>
      <Representation id="lo" bandwidth="64000" audioSamplingRate="44100"/>
      <Representation id="hi" bandwidth="128000" audioSamplingRate="48000"/>
    </AdaptationSet>
    <AdaptationSet contentType="video" mimeType="video/mp4">
      <SegmentTemplate media="v/$Number$.m4s" duration="10"/>
      <Representation id="v" bandwidth="900000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let mpd = parse_mpd(xml).unwrap();
        assert!(!mpd.dynamic);
        let plan = build_plan(&mpd, URL, 0.0).unwrap();
        assert_eq!(plan.sample_rate, Some(48000)); // highest-bandwidth audio rep
        assert_eq!(plan.segments.len(), 3);
        assert_eq!(
            plan.segments[0].url,
            "https://cdn.example.com/dash/hi/seg-1.m4s"
        );
        assert_eq!(
            plan.segments[0].init_url.as_deref(),
            Some("https://cdn.example.com/dash/hi/init.mp4")
        );
        assert_eq!(plan.total_duration, Some(30.0));
        assert!(plan.open_template.is_none());
    }

    #[test]
    fn timeline_template_plan() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT12S">
  <Period>
    <AdaptationSet contentType="audio">
      <Representation id="a" bandwidth="96000" mimeType="audio/mp4">
        <SegmentTemplate initialization="init.mp4" media="s-$Time$.m4s" timescale="1000">
          <SegmentTimeline>
            <S t="0" d="4000" r="1"/>
            <S d="4000"/>
          </SegmentTimeline>
        </SegmentTemplate>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let mpd = parse_mpd(xml).unwrap();
        let plan = build_plan(&mpd, URL, 0.0).unwrap();
        let urls: Vec<&str> = plan.segments.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://cdn.example.com/dash/s-0.m4s",
                "https://cdn.example.com/dash/s-4000.m4s",
                "https://cdn.example.com/dash/s-8000.m4s",
            ]
        );
        assert_eq!(plan.segments[2].key, 8000);
    }

    #[test]
    fn segment_list_and_base_url() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT8S">
  <BaseURL>media/</BaseURL>
  <Period>
    <AdaptationSet contentType="audio">
      <Representation id="a" bandwidth="1">
        <SegmentList>
          <Initialization sourceURL="init.mp4"/>
          <SegmentURL media="s1.m4s"/>
          <SegmentURL media="s2.m4s"/>
        </SegmentList>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let mpd = parse_mpd(xml).unwrap();
        assert_eq!(mpd.base_url.as_deref(), Some("media/"));
        let plan = build_plan(&mpd, URL, 0.0).unwrap();
        assert_eq!(plan.segments.len(), 2);
        assert_eq!(
            plan.segments[0].url,
            "https://cdn.example.com/dash/media/s1.m4s"
        );
        assert_eq!(
            plan.segments[1].init_url.as_deref(),
            Some("https://cdn.example.com/dash/media/init.mp4")
        );
    }

    #[test]
    fn single_file_representation() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT200S">
  <Period>
    <AdaptationSet contentType="audio">
      <Representation id="a" bandwidth="1" mimeType="audio/mpeg">
        <BaseURL>full-track.mp3</BaseURL>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let mpd = parse_mpd(xml).unwrap();
        let plan = build_plan(&mpd, URL, 0.0).unwrap();
        assert_eq!(
            plan.single_file.as_deref(),
            Some("https://cdn.example.com/dash/full-track.mp3")
        );
    }

    #[test]
    fn dynamic_number_template_starts_near_live_edge() {
        let xml = r#"<MPD type="dynamic" availabilityStartTime="1970-01-01T00:00:00Z"
     minimumUpdatePeriod="PT5S">
  <Period start="PT0S">
    <AdaptationSet contentType="audio">
      <SegmentTemplate media="seg-$Number$.aac" duration="4" timescale="1" startNumber="1"/>
      <Representation id="a" bandwidth="64000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let mpd = parse_mpd(xml).unwrap();
        assert!(mpd.dynamic);
        // 100 s since start / 4 s per segment = 25 elapsed → edge 26, back 3.
        let plan = build_plan(&mpd, URL, 100.0).unwrap();
        let ot = plan.open_template.expect("open template");
        assert_eq!(ot.next_number, 23);
        assert_eq!(ot.media_url(23), "https://cdn.example.com/dash/seg-23.aac");
        assert_eq!(ot.segment_seconds, 4.0);
    }

    #[test]
    fn no_audio_errors() {
        let xml = r#"<MPD type="static" mediaPresentationDuration="PT8S">
  <Period>
    <AdaptationSet contentType="video">
      <SegmentTemplate media="v-$Number$.m4s" duration="4"/>
      <Representation id="v" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let mpd = parse_mpd(xml).unwrap();
        assert!(build_plan(&mpd, URL, 0.0).is_err());
    }
}
