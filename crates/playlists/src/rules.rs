use chrono::Utc;
use serde::{Deserialize, Serialize};

// ── Field / Operator / Unit enums ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    All,
    Any,
}

impl Default for MatchType {
    fn default() -> Self {
        MatchType::All
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleField {
    PlayCount,
    SkipCount,
    LastPlayed,
    LastSkipped,
    DateAdded,
    Year,
    Genre,
    Artist,
    Album,
    DurationMs,
    Bitrate,
    IsLiked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleOperator {
    Is,
    IsNot,
    Contains,
    NotContains,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Equals,
    Between,
    InLast,
    NotInLast,
    IsEmpty,
    IsNotEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    Days,
    Weeks,
    Months,
    Years,
}

impl TimeUnit {
    pub fn to_seconds(&self, n: i64) -> i64 {
        match self {
            TimeUnit::Days => n * 86_400,
            TimeUnit::Weeks => n * 604_800,
            TimeUnit::Months => n * 2_592_000,
            TimeUnit::Years => n * 31_536_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    Random,
    PlayCount,
    SkipCount,
    LastPlayed,
    DateAdded,
    Year,
    Title,
    Artist,
    Album,
    DurationMs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Desc
    }
}

// ── Condition / RuleCriteria ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub field: RuleField,
    pub operator: RuleOperator,
    /// Primary value — numeric or string depending on field.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    /// Second value for `between`.
    #[serde(default)]
    pub value2: Option<serde_json::Value>,
    /// Time unit for `in_last` / `not_in_last`.
    #[serde(default)]
    pub unit: Option<TimeUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleCriteria {
    #[serde(default)]
    pub match_type: MatchType,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub limit: Option<usize>,
    pub sort_by: Option<SortField>,
    pub sort_order: Option<SortOrder>,
    /// An RSQL expression (`genre==rock;playcount>5;lastplayed=gt=30d`).
    ///
    /// When present and non-empty it *is* the filter — the structured
    /// `conditions` above are ignored, and matching runs as SQL through the
    /// rsql crate rather than through the in-memory resolver. `limit`,
    /// `sort_by` and `sort_order` still apply. Optional and defaulted so
    /// every stored playlist from before this field existed still
    /// deserializes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rsql: Option<String>,
}

// ── Candidate track fed into the resolver ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub duration_ms: i64,
    pub bitrate: i64,
    pub date_added_ts: i64,
    // stats (default 0 / None when never played)
    pub play_count: i64,
    pub skip_count: i64,
    pub last_played: Option<i64>,
    pub last_skipped: Option<i64>,
    pub is_liked: bool,
}

// ── Resolver ───────────────────────────────────────────────────────────────

pub fn resolve(criteria: &RuleCriteria, mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    let now = Utc::now().timestamp();

    // Filter
    candidates.retain(|c| {
        if criteria.conditions.is_empty() {
            return true;
        }
        let results: Vec<bool> = criteria
            .conditions
            .iter()
            .map(|cond| eval_condition(cond, c, now))
            .collect();
        match criteria.match_type {
            MatchType::All => results.iter().all(|&r| r),
            MatchType::Any => results.iter().any(|&r| r),
        }
    });

    // Sort
    sort_candidates(&mut candidates, criteria, now);

    // Limit
    if let Some(limit) = criteria.limit {
        candidates.truncate(limit);
    }

    candidates
}

fn eval_condition(cond: &Condition, c: &Candidate, now: i64) -> bool {
    match &cond.field {
        RuleField::PlayCount => eval_numeric(cond, c.play_count),
        RuleField::SkipCount => eval_numeric(cond, c.skip_count),
        RuleField::DurationMs => eval_numeric(cond, c.duration_ms),
        RuleField::Bitrate => eval_numeric(cond, c.bitrate),
        RuleField::Year => {
            if let Some(y) = c.year {
                eval_numeric(cond, y)
            } else {
                matches!(cond.operator, RuleOperator::IsEmpty)
            }
        }
        RuleField::LastPlayed => eval_timestamp(cond, c.last_played, now),
        RuleField::LastSkipped => eval_timestamp(cond, c.last_skipped, now),
        RuleField::DateAdded => eval_timestamp(cond, Some(c.date_added_ts), now),
        RuleField::Genre => eval_string(cond, c.genre.as_deref()),
        RuleField::Artist => eval_string(cond, Some(&c.artist)),
        RuleField::Album => eval_string(cond, Some(&c.album)),
        RuleField::IsLiked => {
            let want = cond
                .value
                .as_ref()
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            c.is_liked == want
        }
    }
}

fn eval_numeric(cond: &Condition, val: i64) -> bool {
    let n = cond.value.as_ref().and_then(|v| v.as_i64()).unwrap_or(0);
    let n2 = cond.value2.as_ref().and_then(|v| v.as_i64()).unwrap_or(0);
    match cond.operator {
        RuleOperator::Equals | RuleOperator::Is => val == n,
        RuleOperator::IsNot => val != n,
        RuleOperator::GreaterThan => val > n,
        RuleOperator::LessThan => val < n,
        RuleOperator::GreaterThanOrEqual => val >= n,
        RuleOperator::LessThanOrEqual => val <= n,
        RuleOperator::Between => val >= n && val <= n2,
        _ => false,
    }
}

fn eval_timestamp(cond: &Condition, ts: Option<i64>, now: i64) -> bool {
    match cond.operator {
        RuleOperator::IsEmpty => ts.is_none(),
        RuleOperator::IsNotEmpty => ts.is_some(),
        RuleOperator::InLast => {
            if let (Some(ts), Some(n)) = (ts, cond.value.as_ref().and_then(|v| v.as_i64())) {
                let secs = cond.unit.as_ref().unwrap_or(&TimeUnit::Days).to_seconds(n);
                now - ts <= secs
            } else {
                false
            }
        }
        RuleOperator::NotInLast => {
            if let Some(n) = cond.value.as_ref().and_then(|v| v.as_i64()) {
                let secs = cond.unit.as_ref().unwrap_or(&TimeUnit::Days).to_seconds(n);
                match ts {
                    Some(ts) => now - ts > secs,
                    None => true, // never played → qualifies as "not in last N"
                }
            } else {
                false
            }
        }
        _ => {
            // Treat as numeric comparison on the unix timestamp
            eval_numeric(cond, ts.unwrap_or(0))
        }
    }
}

fn eval_string(cond: &Condition, val: Option<&str>) -> bool {
    let s = val.unwrap_or("");
    let target = cond.value.as_ref().and_then(|v| v.as_str()).unwrap_or("");
    match cond.operator {
        RuleOperator::Is | RuleOperator::Equals => s.eq_ignore_ascii_case(target),
        RuleOperator::IsNot => !s.eq_ignore_ascii_case(target),
        RuleOperator::Contains => s.to_lowercase().contains(&target.to_lowercase()),
        RuleOperator::NotContains => !s.to_lowercase().contains(&target.to_lowercase()),
        RuleOperator::IsEmpty => s.is_empty(),
        RuleOperator::IsNotEmpty => !s.is_empty(),
        _ => false,
    }
}

fn sort_candidates(candidates: &mut Vec<Candidate>, criteria: &RuleCriteria, _now: i64) {
    use rand::seq::SliceRandom;

    let sort_by = criteria.sort_by.as_ref().unwrap_or(&SortField::DateAdded);
    let asc = matches!(
        criteria.sort_order.as_ref().unwrap_or(&SortOrder::Desc),
        SortOrder::Asc
    );

    if matches!(sort_by, SortField::Random) {
        candidates.shuffle(&mut rand::thread_rng());
        return;
    }

    candidates.sort_by(|a, b| {
        let cmp = match sort_by {
            SortField::PlayCount => a.play_count.cmp(&b.play_count),
            SortField::SkipCount => a.skip_count.cmp(&b.skip_count),
            SortField::LastPlayed => a.last_played.unwrap_or(0).cmp(&b.last_played.unwrap_or(0)),
            SortField::DateAdded => a.date_added_ts.cmp(&b.date_added_ts),
            SortField::Year => a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0)),
            SortField::Title => a.title.cmp(&b.title),
            SortField::Artist => a.artist.cmp(&b.artist),
            SortField::Album => a.album.cmp(&b.album),
            SortField::DurationMs => a.duration_ms.cmp(&b.duration_ms),
            SortField::Random => std::cmp::Ordering::Equal,
        };
        if asc {
            cmp
        } else {
            cmp.reverse()
        }
    });
}
