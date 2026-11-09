//! A durable reminder, independent of Thread openness and agent execution.
use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snooze {
    pub started: SystemTime,
    pub until: SystemTime,
}

impl Snooze {
    pub fn should_wake(
        &self,
        thread: crate::ThreadId,
        notices: &crate::notifications::Notifications,
    ) -> bool {
        crate::clock::system_time() >= self.until
            || notices
                .decisions()
                .any(|notice| notice.id.thread == thread && notice.at > self.started)
            || notices.notices().any(|notice| {
                notice.thread == thread
                    && notice.at > self.started
                    && matches!(notice.outcome, crate::TurnOutcome::Error(_))
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Hour,
    ThreeHours,
    Evening,
    Tomorrow,
    Monday,
}

impl Preset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "1 hour",
            Self::ThreeHours => "3 hours",
            Self::Evening => "Today at 6 pm",
            Self::Tomorrow => "Tomorrow at 9 am",
            Self::Monday => "Next Monday at 9 am",
        }
    }
    pub fn until(self, now: SystemTime) -> Option<SystemTime> {
        let local: DateTime<Local> = now.into();
        let (days, hour) = match self {
            Self::Hour => return now.checked_add(Duration::from_secs(3600)),
            Self::ThreeHours => return now.checked_add(Duration::from_secs(10800)),
            Self::Evening => (0, 18),
            Self::Tomorrow => (1, 9),
            Self::Monday => (7 - local.weekday().num_days_from_monday() as i64, 9),
        };
        let date = local
            .date_naive()
            .checked_add_signed(chrono::Duration::days(days))?;
        let until: SystemTime = Local
            .from_local_datetime(&date.and_hms_opt(hour, 0, 0)?)
            .earliest()?
            .into();
        (until > now).then_some(until)
    }
    pub fn available(now: SystemTime) -> Vec<(Self, SystemTime)> {
        let mut presets: Vec<_> = [
            Self::Hour,
            Self::ThreeHours,
            Self::Evening,
            Self::Tomorrow,
            Self::Monday,
        ]
        .into_iter()
        .filter_map(|preset| Some((preset, preset.until(now)?)))
        .collect();
        presets.sort_by_key(|(_, until)| *until);
        presets
    }
}

/// Custom local date/time or a duration: `2026-11-10 14:30`, `90m`, `3h`, `2d`.
pub fn parse(text: &str, now: SystemTime) -> Result<SystemTime, &'static str> {
    let text = text.trim();
    let until =
        if let Some((number, unit)) = text.char_indices().last().map(|(at, _)| text.split_at(at)) {
            match (number.trim().parse::<u64>(), unit) {
                (Ok(n), "m" | "h" | "d") => {
                    let scale = match unit {
                        "m" => 60,
                        "h" => 3600,
                        _ => 86400,
                    };
                    now.checked_add(Duration::from_secs(
                        n.checked_mul(scale).ok_or("Duration is too large")?,
                    ))
                }
                _ => NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
                    .ok()
                    .and_then(|at| Local.from_local_datetime(&at).single())
                    .map(SystemTime::from),
            }
        } else {
            None
        };
    until
        .filter(|until| *until > now)
        .ok_or("Enter a future date and time (YYYY-MM-DD HH:MM), or a duration such as 3h")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_use_local_calendar_times_and_hide_past_options() {
        // A Monday evening: Tomorrow precedes next Monday, and Today is gone.
        let now: SystemTime = Local
            .with_ymd_and_hms(2026, 11, 9, 19, 0, 0)
            .unwrap()
            .into();
        let available = Preset::available(now);
        assert_eq!(
            available
                .iter()
                .map(|(preset, _)| *preset)
                .collect::<Vec<_>>(),
            vec![
                Preset::Hour,
                Preset::ThreeHours,
                Preset::Tomorrow,
                Preset::Monday
            ]
        );
        let tomorrow: DateTime<Local> = Preset::Tomorrow.until(now).unwrap().into();
        let monday: DateTime<Local> = Preset::Monday.until(now).unwrap().into();
        assert_eq!(
            tomorrow.format("%Y-%m-%d %H:%M").to_string(),
            "2026-11-10 09:00"
        );
        assert_eq!(
            monday.format("%Y-%m-%d %H:%M").to_string(),
            "2026-11-16 09:00"
        );
    }

    #[test]
    fn custom_times_accept_durations_and_reject_past_or_overflow() {
        let now: SystemTime = Local
            .with_ymd_and_hms(2026, 11, 9, 12, 0, 0)
            .unwrap()
            .into();
        assert_eq!(parse("90m", now).unwrap(), now + Duration::from_secs(5400));
        assert_eq!(parse("3h", now).unwrap(), now + Duration::from_secs(10800));
        assert_eq!(parse("2d", now).unwrap(), now + Duration::from_secs(172800));
        assert_eq!(
            parse("2026-11-10 09:00", now).unwrap(),
            Preset::Tomorrow.until(now).unwrap()
        );
        for invalid in ["", "0m", "-3h", "18446744073709551615d", "2026-11-09 09:00"] {
            assert!(parse(invalid, now).is_err(), "{invalid}");
        }
    }
}
