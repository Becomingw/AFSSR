use aether_data_contracts::repository::usage::{StoredUsageLeaderboardSummary, UsageActivityRow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Entry {
    pub rank: usize,
    pub id: String,
    pub name: String,
    pub value: Option<f64>,
}
#[derive(Debug, Serialize)]
pub struct Board {
    pub key: &'static str,
    pub unit: &'static str,
    pub items: Vec<Entry>,
}
#[derive(Debug, Serialize)]
pub struct Hour {
    pub hour: i64,
    pub requests: u64,
    pub leaders: Vec<Entry>,
}
#[derive(Debug, Serialize)]
pub struct EarlyBird {
    pub date: String,
    pub winners: Vec<Entry>,
    pub time: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct NightOwl {
    pub date: String,
    pub winners: Vec<Entry>,
}
#[derive(Clone, Copy)]
pub struct LeaderboardWindow {
    pub start_day: i64,
    pub end_day: i64,
    pub offset_minutes: i32,
}
#[derive(Debug, Serialize)]
pub struct LeaderboardReport {
    pub masked: bool,
    pub users: usize,
    pub models: usize,
    pub requests: u64,
    pub tokens: u64,
    pub cost: Option<f64>,
    pub boards: Vec<Board>,
    pub hours: Vec<Hour>,
    pub early_birds: Vec<EarlyBird>,
    pub night_owls: Vec<NightOwl>,
}

fn ranked(mut values: Vec<(String, String, f64)>, hide_values: bool, limit: usize) -> Vec<Entry> {
    values.retain(|(_, _, v)| v.is_finite() && *v > 0.0);
    values.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    let mut rank = 0;
    let mut previous = None;
    values
        .into_iter()
        .take(limit)
        .map(|(id, name, value)| {
            if previous != Some(value) {
                rank += 1;
                previous = Some(value);
            }
            Entry {
                rank,
                id,
                name,
                value: (!hide_values).then_some(value),
            }
        })
        .collect()
}

/// Identity masking happens before serialization, not in the browser.
///
/// In the masked (public) view `names` must only contain users who agreed to be
/// shown by name; everyone else is rendered with an anonymous `成员 NNN` label.
/// Output ids are always opaque `member-N` codes in the masked view.
pub fn build_report(
    users: &[StoredUsageLeaderboardSummary],
    models: &[StoredUsageLeaderboardSummary],
    activity: &[UsageActivityRow],
    names: &BTreeMap<String, String>,
    window: LeaderboardWindow,
    masked: bool,
) -> LeaderboardReport {
    let anchor_day = window.end_day;
    let offset = window.offset_minutes;
    let ids: BTreeSet<_> = users
        .iter()
        .map(|u| u.group_key.clone())
        .chain(
            activity
                .iter()
                .filter(|r| {
                    r.day >= window.start_day
                        && (r.day <= window.end_day || (r.day == window.end_day + 1 && r.hour < 6))
                })
                .map(|r| r.user_id.clone()),
        )
        .collect();
    let labels: BTreeMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let label = if masked {
                names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("成员 {:03}", i + 1))
            } else {
                names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "已删除用户".into())
            };
            let output_id = if masked {
                format!("member-{}", i + 1)
            } else {
                id.clone()
            };
            (id.clone(), (output_id, label))
        })
        .collect();
    let identified = |id: &str, value: f64| {
        let (output_id, name) = labels.get(id).expect("activity identity indexed");
        (output_id.clone(), name.clone(), value)
    };
    let visible_users = !masked || ids.len() >= 3;
    let mut boards = Vec::new();
    for (key, unit, metric) in [
        ("requests", "次", 0),
        ("tokens", "tokens", 1),
        ("cost", "USD", 2),
    ] {
        let values = users
            .iter()
            .map(|u| {
                identified(
                    &u.group_key,
                    match metric {
                        0 => u.request_count as f64,
                        1 => u.total_tokens as f64,
                        _ => u.total_cost_usd,
                    },
                )
            })
            .collect();
        boards.push(Board {
            key,
            unit,
            items: if visible_users {
                ranked(values, masked && metric == 2, 20)
            } else {
                vec![]
            },
        });
    }
    boards.push(Board {
        key: "models",
        unit: "次",
        items: ranked(
            models
                .iter()
                .map(|m| {
                    (
                        m.group_key.clone(),
                        m.group_key.clone(),
                        m.request_count as f64,
                    )
                })
                .collect(),
            false,
            20,
        ),
    });
    let mut daily = BTreeMap::<(String, i64), u64>::new();
    let mut hourly = BTreeMap::<(i64, String), u64>::new();
    let mut first = BTreeMap::<i64, (u64, BTreeSet<String>)>::new();
    let mut active_days = BTreeMap::<String, BTreeSet<i64>>::new();
    let mut nights = BTreeMap::<i64, BTreeMap<String, u64>>::new();
    for r in activity {
        // A night belongs to its 22:00 start date, not its post-midnight date.
        if r.hour >= 22 || r.hour < 6 {
            let night = r.day - i64::from(r.hour < 6);
            if night >= window.start_day && night <= window.end_day {
                *nights
                    .entry(night)
                    .or_default()
                    .entry(r.user_id.clone())
                    .or_default() += r.active_minutes;
            }
        }
        // The extra next-day 00:00-06:00 rows are for the final night only.
        if r.day < window.start_day || r.day > window.end_day {
            continue;
        }
        *daily.entry((r.user_id.clone(), r.day)).or_default() += r.active_minutes;
        *hourly.entry((r.hour, r.user_id.clone())).or_default() += r.requests;
        active_days
            .entry(r.user_id.clone())
            .or_default()
            .insert(r.day);
        if r.hour >= 6 {
            let e = first
                .entry(r.day)
                .or_insert_with(|| (r.first_at, BTreeSet::new()));
            if r.first_at < e.0 {
                *e = (r.first_at, BTreeSet::new());
            }
            if r.first_at == e.0 {
                e.1.insert(r.user_id.clone());
            }
        }
    }
    let week_start = anchor_day - (anchor_day + 3).rem_euclid(7);
    let mut weekly = BTreeMap::<String, u64>::new();
    for ((id, day), minutes) in &daily {
        if *day >= week_start && *day <= anchor_day {
            *weekly.entry(id.clone()).or_default() += minutes;
        }
    }
    let mut early_counts = BTreeMap::<String, u64>::new();
    for (_, winners) in first.values() {
        for id in winners {
            *early_counts.entry(id.clone()).or_default() += 1;
        }
    }
    let mut night_minutes = BTreeMap::<String, u64>::new();
    let mut night_counts = BTreeMap::<String, u64>::new();
    let night_owls = nights
        .into_iter()
        .rev()
        .map(|(day, members)| {
            let max = members.values().copied().max().unwrap_or(0);
            let mut winners = Vec::new();
            for (id, minutes) in members {
                *night_minutes.entry(id.clone()).or_default() += minutes;
                if minutes > 0 && minutes == max {
                    *night_counts.entry(id.clone()).or_default() += 1;
                    if visible_users {
                        let (id, name, value) = identified(&id, minutes as f64);
                        winners.push(Entry {
                            rank: 1,
                            id,
                            name,
                            value: Some(value),
                        });
                    }
                }
            }
            NightOwl {
                date: DateTime::<Utc>::from_timestamp(day * 86400, 0)
                    .unwrap()
                    .format("%Y-%m-%d")
                    .to_string(),
                winners,
            }
        })
        .collect();
    for (key, unit) in [
        ("daily_active", "分钟"),
        ("weekly_active", "分钟"),
        ("active_days", "天"),
        ("streak", "天"),
        ("early_wins", "天"),
        ("night_active", "分钟"),
        ("night_wins", "晚"),
    ] {
        let values = ids
            .iter()
            .map(|id| {
                let value = match key {
                    "daily_active" => *daily.get(&(id.clone(), anchor_day)).unwrap_or(&0),
                    "weekly_active" => *weekly.get(id).unwrap_or(&0),
                    "active_days" => active_days.get(id).map_or(0, |d| d.len() as u64),
                    "early_wins" => *early_counts.get(id).unwrap_or(&0),
                    "night_active" => *night_minutes.get(id).unwrap_or(&0),
                    "night_wins" => *night_counts.get(id).unwrap_or(&0),
                    _ => {
                        let mut last = None;
                        let mut streak = 0;
                        let mut longest = 0;
                        for d in active_days.get(id).into_iter().flatten() {
                            streak = if last == Some(*d - 1) { streak + 1 } else { 1 };
                            longest = longest.max(streak);
                            last = Some(*d);
                        }
                        longest
                    }
                };
                identified(id, value as f64)
            })
            .collect();
        boards.push(Board {
            key,
            unit,
            items: if visible_users {
                ranked(values, false, 20)
            } else {
                vec![]
            },
        });
    }
    let hours = (0..24)
        .map(|hour| {
            let values: Vec<_> = hourly.iter().filter(|((h, _), _)| *h == hour).collect();
            Hour {
                hour,
                requests: values.iter().map(|(_, v)| **v).sum(),
                leaders: if !masked || values.len() >= 3 {
                    ranked(
                        values
                            .into_iter()
                            .map(|((_, id), v)| identified(id, *v as f64))
                            .collect(),
                        false,
                        3,
                    )
                } else {
                    vec![]
                },
            }
        })
        .collect();
    let early_birds = first
        .into_iter()
        .rev()
        .map(|(day, (t, ids))| EarlyBird {
            date: DateTime::<Utc>::from_timestamp(day * 86400, 0)
                .unwrap()
                .format("%Y-%m-%d")
                .to_string(),
            winners: if visible_users {
                ids.into_iter()
                    .map(|id| {
                        let (id, name, _) = identified(&id, 1.0);
                        Entry {
                            rank: 1,
                            id,
                            name,
                            value: None,
                        }
                    })
                    .collect()
            } else {
                vec![]
            },
            time: (!masked).then(|| {
                DateTime::<Utc>::from_timestamp(t as i64 + i64::from(offset) * 60, 0)
                    .unwrap()
                    .format("%H:%M:%S")
                    .to_string()
            }),
        })
        .collect();
    LeaderboardReport {
        masked,
        users: ids.len(),
        models: models.len(),
        requests: models.iter().map(|u| u.request_count).sum(),
        tokens: models.iter().map(|u| u.total_tokens).sum(),
        cost: (!masked).then(|| models.iter().map(|u| u.total_cost_usd).sum()),
        boards,
        hours,
        early_birds,
        night_owls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn early_bird_starts_at_six_and_nights_cross_midnight_without_leaking_into_day_stats() {
        let day = 20000;
        let offset = 480;
        let row = |id: &str, d: i64, hour: i64, minutes: u64, second: i64| UsageActivityRow {
            user_id: id.into(),
            day: d,
            hour,
            requests: 1,
            active_minutes: minutes,
            first_at: (d * 86400 + hour * 3600 + second - i64::from(offset) * 60) as u64,
        };
        let rows = vec![
            row("a", day, 0, 120, 0),
            row("a", day, 5, 5, 3599),
            row("a", day, 6, 5, 60),
            row("b", day, 6, 5, 0),
            row("a", day, 21, 60, 3599),
            row("a", day, 22, 10, 0),
            row("a", day, 23, 15, 0),
            row("a", day + 1, 0, 20, 0),
            row("a", day + 1, 5, 5, 3599),
            row("b", day, 22, 50, 0),
            row("c", day + 1, 6, 120, 0),
            row("c", day, 12, 5, 0),
            row("c", day - 1, 23, 40, 0),
            row("c", day + 1, 22, 120, 0),
        ];
        let window = LeaderboardWindow {
            start_day: day,
            end_day: day,
            offset_minutes: offset,
        };
        let report = build_report(&[], &[], &rows, &BTreeMap::new(), window, false);
        assert_eq!(report.early_birds.len(), 1);
        assert_eq!(report.early_birds[0].time.as_deref(), Some("06:00:00"));
        assert_eq!(report.early_birds[0].winners[0].id, "b");
        assert_eq!(report.night_owls.len(), 1);
        assert_eq!(report.night_owls[0].winners.len(), 2);
        assert!(report.night_owls[0]
            .winners
            .iter()
            .all(|w| w.value == Some(50.0)));
        let board = report
            .boards
            .iter()
            .find(|b| b.key == "night_active")
            .unwrap();
        assert_eq!(board.items.len(), 2);
        assert!(board
            .items
            .iter()
            .all(|e| e.rank == 1 && e.value == Some(50.0)));
        assert_eq!(report.hours[0].requests, 1);
        assert_eq!(report.hours[6].requests, 2);
        assert_eq!(report.hours[23].requests, 1);
        let daily = report
            .boards
            .iter()
            .find(|b| b.key == "daily_active")
            .unwrap();
        assert_eq!(
            daily.items.iter().find(|e| e.id == "a").unwrap().value,
            Some(215.0)
        );
        let public = build_report(&[], &[], &rows, &BTreeMap::new(), window, true);
        let json = serde_json::to_value(&public).unwrap();
        assert!(public.early_birds.iter().all(|d| d.time.is_none()));
        assert_ne!(json["night_owls"][0]["winners"][0]["id"], "a");
        let overnight_only = build_report(
            &[],
            &[],
            &[row("a", day, 5, 5, 0)],
            &BTreeMap::new(),
            window,
            false,
        );
        assert!(overnight_only.early_birds.is_empty());
        assert!(overnight_only.night_owls.is_empty());
    }
    #[test]
    fn leaderboard_masking_ties_and_calendar_week() {
        let users: Vec<_> = ["secret-a", "secret-b", "secret-c"]
            .iter()
            .map(|id| StoredUsageLeaderboardSummary {
                group_key: (*id).into(),
                legacy_name: Some("private-name".into()),
                request_count: 10,
                total_tokens: 100,
                total_cost_usd: 1.234,
            })
            .collect();
        // Epoch day 4 is Monday; day 3 (Sunday) is outside the selected week.
        let rows = vec![
            UsageActivityRow {
                user_id: "secret-a".into(),
                day: 3,
                hour: 8,
                requests: 2,
                active_minutes: 10,
                first_at: 3 * 86400 + 1,
            },
            UsageActivityRow {
                user_id: "secret-a".into(),
                day: 4,
                hour: 8,
                requests: 9,
                active_minutes: 5,
                first_at: 4 * 86400 + 1,
            },
        ];
        let r = build_report(
            &users,
            &[],
            &rows,
            &BTreeMap::new(),
            LeaderboardWindow {
                start_day: 3,
                end_day: 4,
                offset_minutes: 0,
            },
            true,
        );
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("secret-"));
        assert!(!json.contains("private-name"));
        assert!(!json.contains("1.234"));
        assert_eq!(
            r.boards[0].items.iter().map(|e| e.rank).collect::<Vec<_>>(),
            vec![1, 1, 1]
        );
        assert_eq!(
            r.boards
                .iter()
                .find(|b| b.key == "weekly_active")
                .unwrap()
                .items[0]
                .value,
            Some(5.0)
        );
        assert!(r.early_birds.iter().all(|r| r.time.is_none()));
        assert!(r.hours[8].leaders.is_empty());
        let small = build_report(
            &users[..1],
            &[],
            &rows,
            &BTreeMap::new(),
            LeaderboardWindow {
                start_day: 3,
                end_day: 4,
                offset_minutes: 0,
            },
            true,
        );
        assert!(small.boards.iter().all(|b| b.items.is_empty()));
    }
    #[test]
    fn masked_view_shows_public_names_but_keeps_ids_opaque() {
        let users: Vec<_> = ["secret-a", "secret-b", "secret-c"]
            .iter()
            .map(|id| StoredUsageLeaderboardSummary {
                group_key: (*id).into(),
                legacy_name: None,
                request_count: 10,
                total_tokens: 100,
                total_cost_usd: 1.234,
            })
            .collect();
        // secret-b opted into anonymity, so the caller does not pass its name.
        let names = BTreeMap::from([
            ("secret-a".to_string(), "alice".to_string()),
            ("secret-c".to_string(), "carol".to_string()),
        ]);
        let r = build_report(
            &users,
            &[],
            &[],
            &names,
            LeaderboardWindow {
                start_day: 3,
                end_day: 4,
                offset_minutes: 0,
            },
            true,
        );
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("secret-"));
        assert!(!json.contains("1.234"));
        let mut labels: Vec<_> = r.boards[0].items.iter().map(|e| e.name.clone()).collect();
        labels.sort();
        assert_eq!(labels, vec!["alice", "carol", "成员 002"]);
        assert!(r.boards[0]
            .items
            .iter()
            .all(|e| e.id.starts_with("member-")));
    }
}
