//! Cron expression parser and scheduler.
//!
//! Supports standard five-field cron expressions (minute hour dom month dow),
//! with comma-separated values, ranges (`-`), steps (`/`), and wildcards (`*`).

use chrono::{DateTime, Datelike, Timelike, Utc};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CronError {
    #[error("invalid field '{field}': {reason}")]
    InvalidField { field: String, reason: String },
    #[error("expression must have 5 fields, got {got}: '{raw}'")]
    WrongFieldCount { got: usize, raw: String },
}

/// A single field value in a cron expression — either a wildcard or a set of valid numbers.
#[derive(Debug, Clone, PartialEq)]
enum Field {
    Any,
    Set(Vec<u32>),
}

impl Field {
    /// Parse a single cron field (e.g. `*`, `5`, `1,3,5`, `0-6`, `*/15`).
    fn parse(raw: &str, min: u32, max: u32) -> Result<Self, String> {
        let raw = raw.trim();
        if raw == "*" {
            return Ok(Field::Any);
        }

        let mut values = Vec::new();

        for part in raw.split(',') {
            let part = part.trim();

            if let Some(slash_pos) = part.find('/') {
                // Step syntax: `*/15` or `1-10/2`
                let (range_str, step_str) = part.split_at(slash_pos);
                let step: u32 = step_str[1..]
                    .parse()
                    .map_err(|_| format!("invalid step: {}", &step_str[1..]))?;
                let range = if range_str == "*" {
                    min..=max
                } else if let Some(dash_pos) = range_str.find('-') {
                    let (lo_str, hi_str) = range_str.split_at(dash_pos);
                    let lo: u32 = lo_str
                        .parse()
                        .map_err(|_| format!("invalid range start: {lo_str}"))?;
                    let hi: u32 = hi_str[1..]
                        .parse()
                        .map_err(|_| format!("invalid range end: {}", &hi_str[1..]))?;
                    lo..=hi
                } else {
                    return Err(format!("invalid step range: {range_str}"));
                };
                for v in (*range.start()..=*range.end()).step_by(step as usize) {
                    if v >= min && v <= max {
                        values.push(v);
                    }
                }
            } else if let Some(dash_pos) = part.find('-') {
                // Range syntax: `1-5`
                let (lo_str, hi_str) = part.split_at(dash_pos);
                let lo: u32 = lo_str
                    .parse()
                    .map_err(|_| format!("invalid range start: {lo_str}"))?;
                let hi: u32 = hi_str[1..]
                    .parse()
                    .map_err(|_| format!("invalid range end: {}", &hi_str[1..]))?;
                for v in lo..=hi {
                    if v >= min && v <= max {
                        values.push(v);
                    }
                }
            } else {
                let v: u32 = part.parse().map_err(|_| format!("invalid value: {part}"))?;
                if v < min || v > max {
                    return Err(format!("value {v} out of range [{min}, {max}]"));
                }
                values.push(v);
            }
        }

        values.sort();
        values.dedup();
        Ok(Field::Set(values))
    }

    /// Does this field match the given value?
    fn matches(&self, value: u32) -> bool {
        match self {
            Field::Any => true,
            Field::Set(ref vals) => vals.contains(&value),
        }
    }
}

/// A parsed standard five-field cron expression.
#[derive(Debug, Clone)]
pub struct CronExpr {
    minute: Field,
    hour: Field,
    dom: Field,
    month: Field,
    dow: Field,
}

impl CronExpr {
    /// Parse from a standard five-field string.
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(CronError::WrongFieldCount {
                got: parts.len(),
                raw: expr.to_string(),
            });
        }
        Ok(Self {
            minute: Field::parse(parts[0], 0, 59).map_err(|r| CronError::InvalidField {
                field: "minute".into(),
                reason: r,
            })?,
            hour: Field::parse(parts[1], 0, 23).map_err(|r| CronError::InvalidField {
                field: "hour".into(),
                reason: r,
            })?,
            dom: Field::parse(parts[2], 1, 31).map_err(|r| CronError::InvalidField {
                field: "dom".into(),
                reason: r,
            })?,
            month: Field::parse(parts[3], 1, 12).map_err(|r| CronError::InvalidField {
                field: "month".into(),
                reason: r,
            })?,
            dow: Field::parse(parts[4], 0, 7).map_err(|r| CronError::InvalidField {
                field: "dow".into(),
                reason: r,
            })?,
        })
    }

    /// Does this expression match the given timestamp?
    pub fn matches(&self, dt: &DateTime<Utc>) -> bool {
        let minute = dt.minute();
        let hour = dt.hour();
        let dom = dt.day();
        let month = dt.month();
        // chrono: Mon=1..Sun=7; cron dow: 0/7=Sun, 1=Mon..6=Sat
        let weekday_num = dt.weekday().num_days_from_sunday();
        let dow_val = if weekday_num == 0 { 0 } else { weekday_num };

        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.dom.matches(dom)
            && self.month.matches(month)
            && (self.dow.matches(dow_val) || (dow_val == 7 && self.dow.matches(0)))
    }

    /// Human-readable description.
    pub fn display(&self) -> String {
        format!(
            "min={:?} hour={:?} dom={:?} month={:?} dow={:?}",
            self.minute, self.hour, self.dom, self.month, self.dow
        )
    }
}

/// A scheduled task tied to a cron expression.
#[derive(Debug, Clone)]
pub struct CronTask {
    pub name: String,
    pub expr: String,
    pub parsed: CronExpr,
    pub goal_description: String,
    pub goal_priority: u8,
}

impl CronTask {
    pub fn new(name: &str, expr: &str, goal_desc: &str, priority: u8) -> Result<Self, CronError> {
        let parsed = CronExpr::parse(expr)?;
        Ok(Self {
            name: name.to_string(),
            expr: expr.to_string(),
            parsed,
            goal_description: goal_desc.to_string(),
            goal_priority: priority,
        })
    }

    pub fn should_trigger(&self, now: &DateTime<Utc>) -> bool {
        self.parsed.matches(now)
    }
}

/// A scheduler that checks registered cron tasks against the current time
/// and triggers those that match.
pub struct CronScheduler {
    tasks: Vec<CronTask>,
    last_trigger: std::collections::HashMap<String, DateTime<Utc>>,
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            last_trigger: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, task: CronTask) {
        self.tasks.push(task);
    }

    /// Returns the list of tasks that are due to trigger now (not yet fired this minute).
    pub fn tick(&mut self, now: &DateTime<Utc>) -> Vec<&CronTask> {
        let minute_key = format!(
            "{}-{:02}-{:02}T{:02}:{:02}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute()
        );

        if self.last_trigger.contains_key(&minute_key) {
            return vec![];
        }

        let mut triggered = Vec::new();
        for task in &self.tasks {
            if task.should_trigger(now) {
                triggered.push(task);
            }
        }

        self.last_trigger.insert(minute_key, *now);
        // Prune old keys to avoid unbounded growth
        if self.last_trigger.len() > 1440 {
            self.last_trigger
                .retain(|_, v| now.signed_duration_since(*v).num_minutes() < 120);
        }

        triggered
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(dom: u32, hour: u32, min: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 6, dom, hour, min, 0).unwrap()
    }

    #[test]
    fn parse_every_minute() {
        let e = CronExpr::parse("* * * * *").unwrap();
        assert!(e.matches(&dt(16, 10, 30)));
        assert!(e.matches(&dt(16, 0, 0)));
        assert!(e.matches(&dt(1, 23, 59)));
    }

    #[test]
    fn parse_specific_time() {
        let e = CronExpr::parse("30 8 * * *").unwrap(); // 08:30 daily
        assert!(e.matches(&dt(16, 8, 30)));
        assert!(!e.matches(&dt(16, 8, 31)));
        assert!(!e.matches(&dt(16, 9, 30)));
    }

    #[test]
    fn parse_hourly() {
        let e = CronExpr::parse("0 * * * *").unwrap(); // every hour at :00
        assert!(e.matches(&dt(16, 0, 0)));
        assert!(e.matches(&dt(16, 10, 0)));
        assert!(!e.matches(&dt(16, 10, 1)));
    }

    #[test]
    fn parse_every_six_hours() {
        let e = CronExpr::parse("0 */6 * * *").unwrap(); // every 6 hours
        assert!(e.matches(&dt(16, 0, 0)));
        assert!(e.matches(&dt(16, 6, 0)));
        assert!(e.matches(&dt(16, 12, 0)));
        assert!(e.matches(&dt(16, 18, 0)));
        assert!(!e.matches(&dt(16, 3, 0)));
        assert!(!e.matches(&dt(16, 1, 0)));
    }

    #[test]
    fn parse_range() {
        let e = CronExpr::parse("0 9-17 * * 1-5").unwrap(); // work hours weekdays
        assert!(e.matches(&dt(16, 9, 0))); // Tue Jun 16 2026 is a Tuesday (2)
        assert!(e.matches(&dt(16, 17, 0))); // Tue
        assert!(!e.matches(&dt(14, 9, 0))); // Sun
        assert!(!e.matches(&dt(16, 8, 0))); // before 9am
        assert!(!e.matches(&dt(16, 18, 0))); // after 5pm
    }

    #[test]
    fn parse_midnight() {
        let e = CronExpr::parse("0 0 * * *").unwrap();
        assert!(e.matches(&dt(16, 0, 0)));
        assert!(!e.matches(&dt(16, 0, 1)));
    }

    #[test]
    fn parse_comma_list() {
        let e = CronExpr::parse("0 6,12,18 * * *").unwrap();
        assert!(e.matches(&dt(16, 6, 0)));
        assert!(e.matches(&dt(16, 12, 0)));
        assert!(e.matches(&dt(16, 18, 0)));
        assert!(!e.matches(&dt(16, 9, 0)));
    }

    #[test]
    fn parse_step_short_range() {
        // Every 15 minutes
        let e = CronExpr::parse("*/15 * * * *").unwrap();
        assert!(e.matches(&dt(16, 10, 0)));
        assert!(e.matches(&dt(16, 10, 15)));
        assert!(e.matches(&dt(16, 10, 30)));
        assert!(e.matches(&dt(16, 10, 45)));
        assert!(!e.matches(&dt(16, 10, 7)));
    }

    #[test]
    fn parse_dow_sunday() {
        // 0 = Sunday in cron, chrono Weekday::Sun = 7
        let e = CronExpr::parse("0 12 * * 0").unwrap();
        let sun = chrono::NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let sun_dt: DateTime<Utc> =
            DateTime::from_naive_utc_and_offset(sun.and_hms_opt(12, 0, 0).unwrap(), Utc);
        assert!(e.matches(&sun_dt)); // Sunday
        assert!(!e.matches(&dt(16, 12, 0))); // Tuesday
    }

    #[test]
    fn parse_invalid_too_few() {
        assert!(CronExpr::parse("* * * *").is_err());
    }

    #[test]
    fn parse_invalid_too_many() {
        assert!(CronExpr::parse("* * * * * *").is_err());
    }

    #[test]
    fn parse_invalid_minute() {
        assert!(CronExpr::parse("60 * * * *").is_err());
    }

    #[test]
    fn cron_task_should_trigger() {
        let task = CronTask::new("test", "0 10 * * *", "daily check", 5).unwrap();
        assert!(task.should_trigger(&dt(16, 10, 0)));
        assert!(!task.should_trigger(&dt(16, 10, 1)));
    }

    #[test]
    fn scheduler_tick_triggers_once_per_minute() {
        let mut sched = CronScheduler::new();
        sched.register(CronTask::new("a", "0 10 * * *", "goal", 1).unwrap());
        sched.register(CronTask::new("b", "30 * * * *", "goal", 2).unwrap());

        let t1 = dt(16, 10, 0);
        let triggered = sched.tick(&t1);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].name, "a");

        // Same minute — no new triggers
        let t1b = dt(16, 10, 0);
        assert!(sched.tick(&t1b).is_empty());

        // Different minute
        let t2 = dt(16, 10, 30);
        let triggered2 = sched.tick(&t2);
        assert_eq!(triggered2.len(), 1);
        assert_eq!(triggered2[0].name, "b");
    }

    #[test]
    fn scheduler_prunes_old_triggers() {
        let mut sched = CronScheduler::new();
        sched.register(CronTask::new("x", "* * * * *", "every minute", 1).unwrap());
        // Simulate many ticks
        for h in 0..24 {
            for m in 0..60 {
                let t = dt(16, h, m);
                sched.tick(&t);
            }
        }
        // Should still work after pruning
        assert!(sched.last_trigger.len() < 2000);
        let triggered = sched.tick(&dt(17, 12, 0));
        assert_eq!(triggered.len(), 1);
    }
}
