//! Retention policy implementation for snapshot compaction.
//!
//! Uses Grandfather/Father/Son (GFS) rotation scheme:
//! - Hourly tier: Keep N snapshots from the last 24 hours
//! - Daily tier: Keep N snapshots, one per day for the last week
//! - Weekly tier: Keep N snapshots, one per week for the last 12 weeks
//! - Monthly tier: Keep N snapshots, one per month beyond 12 weeks

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// Retention policy configuration
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Number of hourly snapshots to keep (default: 24)
    pub hourly: usize,
    /// Number of daily snapshots to keep (default: 7)
    pub daily: usize,
    /// Number of weekly snapshots to keep (default: 12)
    pub weekly: usize,
    /// Number of monthly snapshots to keep (default: 12)
    pub monthly: usize,
    /// Minimum total snapshots to keep (safety floor)
    pub minimum: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            hourly: 24,
            daily: 7,
            weekly: 12,
            monthly: 12,
            minimum: 2,
        }
    }
}

impl RetentionPolicy {
    /// Create a new policy with custom values
    pub fn new(hourly: usize, daily: usize, weekly: usize, monthly: usize) -> Self {
        Self {
            hourly,
            daily,
            weekly,
            monthly,
            minimum: 2,
        }
    }
}

/// Tier classification for snapshots
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Less than 24 hours old
    Hourly,
    /// 24 hours to 7 days old
    Daily,
    /// 7 days to 12 weeks old
    Weekly,
    /// More than 12 weeks old
    Monthly,
}

impl Tier {
    /// Classify a snapshot based on its age
    pub fn classify(now: DateTime<Utc>, created_at: DateTime<Utc>) -> Self {
        let age = now.signed_duration_since(created_at);

        if age < Duration::hours(24) {
            Tier::Hourly
        } else if age < Duration::days(7) {
            Tier::Daily
        } else if age < Duration::weeks(12) {
            Tier::Weekly
        } else {
            Tier::Monthly
        }
    }
}

/// A snapshot entry for retention analysis
#[derive(Debug, Clone)]
pub struct SnapshotEntry {
    /// Filename (S3 key or local filename)
    pub filename: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Maximum transaction ID (for determining latest)
    pub max_txid: u64,
    /// File size in bytes
    pub size: u64,
}

/// Result of compaction analysis
#[derive(Debug, Clone)]
pub struct CompactionPlan {
    /// Snapshots to keep
    pub keep: Vec<SnapshotEntry>,
    /// Snapshots to delete
    pub delete: Vec<SnapshotEntry>,
    /// Bytes that will be freed
    pub bytes_freed: u64,
}

impl CompactionPlan {
    /// Check if there's anything to delete
    pub fn has_deletions(&self) -> bool {
        !self.delete.is_empty()
    }

    /// Get a summary of the plan
    pub fn summary(&self) -> String {
        format!(
            "Keep: {} snapshots, Delete: {} snapshots, Free: {} bytes ({:.2} MB)",
            self.keep.len(),
            self.delete.len(),
            self.bytes_freed,
            self.bytes_freed as f64 / (1024.0 * 1024.0)
        )
    }
}

/// Generate a bucket key for grouping snapshots by time period
fn bucket_key(tier: Tier, created_at: DateTime<Utc>) -> String {
    match tier {
        Tier::Hourly => {
            // Group by hour: "2024-01-15T10"
            created_at.format("%Y-%m-%dT%H").to_string()
        }
        Tier::Daily => {
            // Group by day: "2024-01-15"
            created_at.format("%Y-%m-%d").to_string()
        }
        Tier::Weekly => {
            // Group by ISO week: "2024-W03"
            created_at.format("%Y-W%W").to_string()
        }
        Tier::Monthly => {
            // Group by month: "2024-01"
            created_at.format("%Y-%m").to_string()
        }
    }
}

/// Analyze snapshots and determine which to keep/delete based on retention policy
///
/// Algorithm:
/// 1. Always keep the latest snapshot (safety)
/// 2. Classify each snapshot into tiers based on age
/// 3. Within each tier, group by time bucket (hour/day/week/month)
/// 4. Keep the latest snapshot from each bucket, up to the tier limit
/// 5. Ensure minimum total is maintained
pub fn analyze_retention(
    snapshots: &[SnapshotEntry],
    policy: &RetentionPolicy,
    now: DateTime<Utc>,
) -> CompactionPlan {
    if snapshots.is_empty() {
        return CompactionPlan {
            keep: vec![],
            delete: vec![],
            bytes_freed: 0,
        };
    }

    // Find the latest snapshot - always keep it
    let latest = snapshots
        .iter()
        .max_by_key(|s| s.max_txid)
        .expect("snapshots not empty");

    let mut keep_set: HashMap<String, bool> = HashMap::new();
    keep_set.insert(latest.filename.clone(), true);

    // Group snapshots by tier and bucket
    let mut hourly_buckets: HashMap<String, Vec<&SnapshotEntry>> = HashMap::new();
    let mut daily_buckets: HashMap<String, Vec<&SnapshotEntry>> = HashMap::new();
    let mut weekly_buckets: HashMap<String, Vec<&SnapshotEntry>> = HashMap::new();
    let mut monthly_buckets: HashMap<String, Vec<&SnapshotEntry>> = HashMap::new();

    for snapshot in snapshots {
        let tier = Tier::classify(now, snapshot.created_at);
        let key = bucket_key(tier, snapshot.created_at);

        match tier {
            Tier::Hourly => hourly_buckets.entry(key).or_default().push(snapshot),
            Tier::Daily => daily_buckets.entry(key).or_default().push(snapshot),
            Tier::Weekly => weekly_buckets.entry(key).or_default().push(snapshot),
            Tier::Monthly => monthly_buckets.entry(key).or_default().push(snapshot),
        }
    }

    // Select best snapshot from each bucket (highest TXID)
    // Then keep up to the limit for each tier
    fn select_from_buckets(
        buckets: HashMap<String, Vec<&SnapshotEntry>>,
        limit: usize,
        keep_set: &mut HashMap<String, bool>,
    ) {
        // Get best from each bucket
        let mut best: Vec<&SnapshotEntry> = buckets
            .into_values()
            .filter_map(|mut entries| {
                entries.sort_by_key(|e| std::cmp::Reverse(e.max_txid));
                entries.into_iter().next()
            })
            .collect();

        // Sort by TXID descending (newest first)
        best.sort_by_key(|e| std::cmp::Reverse(e.max_txid));

        // Keep up to limit
        for entry in best.into_iter().take(limit) {
            keep_set.insert(entry.filename.clone(), true);
        }
    }

    select_from_buckets(hourly_buckets, policy.hourly, &mut keep_set);
    select_from_buckets(daily_buckets, policy.daily, &mut keep_set);
    select_from_buckets(weekly_buckets, policy.weekly, &mut keep_set);
    select_from_buckets(monthly_buckets, policy.monthly, &mut keep_set);

    // Ensure minimum is met - if not enough kept, keep oldest ones too
    if keep_set.len() < policy.minimum {
        let mut all_sorted: Vec<&SnapshotEntry> = snapshots.iter().collect();
        all_sorted.sort_by_key(|s| s.max_txid);

        for snapshot in all_sorted {
            if keep_set.len() >= policy.minimum {
                break;
            }
            keep_set.insert(snapshot.filename.clone(), true);
        }
    }

    // Split into keep and delete lists
    let mut keep = Vec::new();
    let mut delete = Vec::new();
    let mut bytes_freed = 0;

    for snapshot in snapshots {
        if keep_set.contains_key(&snapshot.filename) {
            keep.push(snapshot.clone());
        } else {
            bytes_freed += snapshot.size;
            delete.push(snapshot.clone());
        }
    }

    // Sort keep by TXID descending for display
    keep.sort_by_key(|s| std::cmp::Reverse(s.max_txid));
    delete.sort_by_key(|s| std::cmp::Reverse(s.max_txid));

    CompactionPlan {
        keep,
        delete,
        bytes_freed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(filename: &str, hours_ago: i64, txid: u64, size: u64) -> SnapshotEntry {
        let now = Utc::now();
        SnapshotEntry {
            filename: filename.to_string(),
            created_at: now - Duration::hours(hours_ago),
            max_txid: txid,
            size,
        }
    }

    fn make_snapshot_days(filename: &str, days_ago: i64, txid: u64, size: u64) -> SnapshotEntry {
        let now = Utc::now();
        SnapshotEntry {
            filename: filename.to_string(),
            created_at: now - Duration::days(days_ago),
            max_txid: txid,
            size,
        }
    }

    #[test]
    fn test_tier_classification() {
        let now = Utc::now();

        // Hourly: less than 24 hours
        let hourly = now - Duration::hours(1);
        assert_eq!(Tier::classify(now, hourly), Tier::Hourly);

        let hourly_edge = now - Duration::hours(23);
        assert_eq!(Tier::classify(now, hourly_edge), Tier::Hourly);

        // Daily: 24 hours to 7 days
        let daily = now - Duration::hours(25);
        assert_eq!(Tier::classify(now, daily), Tier::Daily);

        let daily_edge = now - Duration::days(6);
        assert_eq!(Tier::classify(now, daily_edge), Tier::Daily);

        // Weekly: 7 days to 12 weeks
        let weekly = now - Duration::days(8);
        assert_eq!(Tier::classify(now, weekly), Tier::Weekly);

        let weekly_edge = now - Duration::weeks(11);
        assert_eq!(Tier::classify(now, weekly_edge), Tier::Weekly);

        // Monthly: more than 12 weeks
        let monthly = now - Duration::weeks(13);
        assert_eq!(Tier::classify(now, monthly), Tier::Monthly);
    }

    #[test]
    fn test_empty_snapshots() {
        let policy = RetentionPolicy::default();
        let plan = analyze_retention(&[], &policy, Utc::now());

        assert!(plan.keep.is_empty());
        assert!(plan.delete.is_empty());
        assert_eq!(plan.bytes_freed, 0);
    }

    #[test]
    fn test_single_snapshot_always_kept() {
        let now = Utc::now();
        let snapshots = vec![make_snapshot("snapshot1.ltx", 1, 100, 1000)];

        let policy = RetentionPolicy::default();
        let plan = analyze_retention(&snapshots, &policy, now);

        assert_eq!(plan.keep.len(), 1);
        assert!(plan.delete.is_empty());
        assert_eq!(plan.bytes_freed, 0);
    }

    #[test]
    fn test_minimum_snapshots_enforced() {
        let now = Utc::now();
        // Create 3 snapshots
        let snapshots = vec![
            make_snapshot("s1.ltx", 1, 1, 1000),
            make_snapshot("s2.ltx", 2, 2, 1000),
            make_snapshot("s3.ltx", 3, 3, 1000),
        ];

        // Policy with very restrictive limits but minimum of 2
        let policy = RetentionPolicy {
            hourly: 1,
            daily: 0,
            weekly: 0,
            monthly: 0,
            minimum: 2,
        };

        let plan = analyze_retention(&snapshots, &policy, now);

        // Should keep at least 2 (minimum)
        assert!(plan.keep.len() >= 2);
    }

    #[test]
    fn test_latest_always_kept() {
        let now = Utc::now();
        let snapshots = vec![
            make_snapshot("old.ltx", 100, 1, 1000),
            make_snapshot("new.ltx", 1, 100, 1000),
        ];

        let policy = RetentionPolicy {
            hourly: 0,
            daily: 0,
            weekly: 0,
            monthly: 0,
            minimum: 1,
        };

        let plan = analyze_retention(&snapshots, &policy, now);

        // Latest (highest TXID) must be kept
        assert!(plan.keep.iter().any(|s| s.filename == "new.ltx"));
    }

    #[test]
    fn test_hourly_bucketing() {
        let now = Utc::now();

        // Create multiple snapshots in the same hour
        let snapshots = vec![
            make_snapshot("h1_a.ltx", 0, 10, 1000), // same hour
            make_snapshot("h1_b.ltx", 0, 20, 1000), // same hour, higher TXID
            make_snapshot("h2.ltx", 2, 30, 1000),   // different hour
            make_snapshot("h3.ltx", 3, 40, 1000),   // different hour
        ];

        let policy = RetentionPolicy {
            hourly: 2,
            daily: 0,
            weekly: 0,
            monthly: 0,
            minimum: 1,
        };

        let plan = analyze_retention(&snapshots, &policy, now);

        // Should keep the highest TXID from each bucket, up to 2
        // h1_b (txid 20) and h3 (txid 40) - the latest from each of 2 buckets
        // Note: with minimum=1 and hourly=2, we keep the latest + best from other buckets
        assert!(plan.keep.len() >= 2);
        assert!(plan.keep.iter().any(|s| s.filename == "h3.ltx"));
    }

    #[test]
    fn test_daily_bucketing() {
        let now = Utc::now();

        // Create snapshots spanning multiple days (but older than 24 hours)
        let snapshots = vec![
            make_snapshot_days("d1_a.ltx", 2, 10, 1000),
            make_snapshot_days("d1_b.ltx", 2, 20, 1000), // same day
            make_snapshot_days("d2.ltx", 3, 30, 1000),
            make_snapshot_days("d3.ltx", 4, 40, 1000),
        ];

        let policy = RetentionPolicy {
            hourly: 0,
            daily: 2,
            weekly: 0,
            monthly: 0,
            minimum: 1,
        };

        let plan = analyze_retention(&snapshots, &policy, now);

        // Should keep latest (d3) + one more daily bucket
        assert!(plan.keep.len() >= 2);
        assert!(plan.keep.iter().any(|s| s.filename == "d3.ltx"));
    }

    #[test]
    fn test_realistic_scenario() {
        let now = Utc::now();

        // Simulate 50 snapshots over several months
        let mut snapshots = Vec::new();

        // Recent hourly snapshots (last 24 hours)
        for i in 0..24 {
            snapshots.push(make_snapshot(
                &format!("hourly_{}.ltx", i),
                i,
                100 + i as u64,
                10000,
            ));
        }

        // Daily snapshots (last week)
        for i in 1..8 {
            snapshots.push(make_snapshot_days(
                &format!("daily_{}.ltx", i),
                i,
                50 + i as u64,
                10000,
            ));
        }

        // Weekly snapshots (last 12 weeks)
        for i in 2..14 {
            snapshots.push(SnapshotEntry {
                filename: format!("weekly_{}.ltx", i),
                created_at: now - Duration::weeks(i),
                max_txid: 20 + i as u64,
                size: 10000,
            });
        }

        // Monthly snapshots (older than 12 weeks)
        for i in 4..10 {
            snapshots.push(SnapshotEntry {
                filename: format!("monthly_{}.ltx", i),
                created_at: now - Duration::weeks(12 + i * 4),
                max_txid: i as u64,
                size: 10000,
            });
        }

        let policy = RetentionPolicy::default(); // 24 hourly, 7 daily, 12 weekly, 12 monthly
        let plan = analyze_retention(&snapshots, &policy, now);

        // Should delete some snapshots
        assert!(plan.has_deletions());

        // Should keep reasonable number (close to policy limits)
        // Latest + hourly + daily + weekly + monthly should be around 24+7+12+12 = 55 max
        // But we have 50+ snapshots with overlap, so should keep fewer
        assert!(plan.keep.len() <= 60);
        assert!(plan.keep.len() >= 20);

        // Latest must be kept
        let max_txid = snapshots.iter().map(|s| s.max_txid).max().unwrap();
        assert!(plan.keep.iter().any(|s| s.max_txid == max_txid));

        // Summary should be informative
        let summary = plan.summary();
        assert!(summary.contains("Keep:"));
        assert!(summary.contains("Delete:"));
        assert!(summary.contains("Free:"));
    }

    #[test]
    fn test_bucket_key_format() {
        let dt = DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(bucket_key(Tier::Hourly, dt), "2024-01-15T10");
        assert_eq!(bucket_key(Tier::Daily, dt), "2024-01-15");
        assert_eq!(bucket_key(Tier::Monthly, dt), "2024-01");
    }

    #[test]
    fn test_bytes_freed_calculation() {
        let now = Utc::now();
        let snapshots = vec![
            make_snapshot("keep.ltx", 1, 100, 5000),
            make_snapshot("delete1.ltx", 2, 50, 3000),
            make_snapshot("delete2.ltx", 3, 30, 2000),
        ];

        let policy = RetentionPolicy {
            hourly: 1,
            daily: 0,
            weekly: 0,
            monthly: 0,
            minimum: 1,
        };

        let plan = analyze_retention(&snapshots, &policy, now);

        // The bytes freed should equal the size of deleted snapshots
        let deleted_size: u64 = plan.delete.iter().map(|s| s.size).sum();
        assert_eq!(plan.bytes_freed, deleted_size);
    }
}
