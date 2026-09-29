use super::*;
use crate::store::memory::MemoryStore;

const NOW: u64 = 1_800_000_000;

fn limits(ip: u32, username: u32, pair: u32) -> Limits {
    Limits {
        ip_per_window: ip,
        username_per_window: username,
        pair_per_window: pair,
        ..Limits::default()
    }
}

fn user(name: &str) -> Username {
    name.parse().unwrap()
}

fn is_allowed(verdict: Verdict) -> bool {
    matches!(verdict, Verdict::Allowed { .. })
}

#[tokio::test]
async fn counts_down_remaining_and_reports_window_end() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(100, 100, 3));
    let window_end = NOW / 60 * 60 + 60;

    assert_eq!(
        limiter.check("1.1.1.1", &user("a"), NOW).await,
        Verdict::Allowed {
            remaining: 2,
            reset: window_end
        }
    );
    assert_eq!(
        limiter.check("1.1.1.1", &user("a"), NOW).await,
        Verdict::Allowed {
            remaining: 1,
            reset: window_end
        }
    );
}

#[tokio::test]
async fn blocks_a_pair_once_it_exceeds_the_limit_and_keeps_it_blocked() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(100, 100, 2));
    let a = user("a");

    assert!(is_allowed(limiter.check("1.1.1.1", &a, NOW).await));
    assert!(is_allowed(limiter.check("1.1.1.1", &a, NOW).await));
    assert_eq!(limiter.check("1.1.1.1", &a, NOW).await, Verdict::Blocked);
    assert_eq!(
        limiter.check("1.1.1.1", &a, NOW + 120).await,
        Verdict::Blocked
    );
    assert_eq!(store.ttl("block:1.1.1.1:a"), Some(300));
}

#[tokio::test]
async fn a_new_window_resets_the_pair_count() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(100, 100, 2));
    let a = user("a");

    assert!(is_allowed(limiter.check("1.1.1.1", &a, NOW).await));
    assert_eq!(
        limiter.check("1.1.1.1", &a, NOW + 60).await,
        Verdict::Allowed {
            remaining: 1,
            reset: (NOW + 60) / 60 * 60 + 60
        }
    );
}

#[tokio::test]
async fn blocking_one_pair_does_not_affect_others() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(100, 100, 1));

    assert!(is_allowed(limiter.check("1.1.1.1", &user("a"), NOW).await));
    assert_eq!(
        limiter.check("1.1.1.1", &user("a"), NOW).await,
        Verdict::Blocked
    );
    assert!(is_allowed(limiter.check("1.1.1.1", &user("b"), NOW).await));
    assert!(is_allowed(limiter.check("2.2.2.2", &user("a"), NOW).await));
}

#[tokio::test]
async fn limits_total_requests_per_ip() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(2, 100, 100));

    assert!(is_allowed(limiter.check("1.1.1.1", &user("a"), NOW).await));
    assert!(is_allowed(limiter.check("1.1.1.1", &user("b"), NOW).await));
    assert_eq!(
        limiter.check("1.1.1.1", &user("c"), NOW).await,
        Verdict::IpLimited
    );
    assert!(is_allowed(limiter.check("2.2.2.2", &user("c"), NOW).await));
}

#[tokio::test]
async fn limits_total_requests_per_username_across_ips() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(100, 2, 100));
    let popular = user("popular");

    assert!(is_allowed(limiter.check("1.1.1.1", &popular, NOW).await));
    assert!(is_allowed(limiter.check("2.2.2.2", &popular, NOW).await));
    assert_eq!(
        limiter.check("3.3.3.3", &popular, NOW).await,
        Verdict::UsernameLimited
    );
}

#[tokio::test]
async fn ip_limit_is_reported_before_username_limit() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, limits(1, 1, 100));
    let a = user("a");

    assert!(is_allowed(limiter.check("1.1.1.1", &a, NOW).await));
    assert_eq!(limiter.check("1.1.1.1", &a, NOW).await, Verdict::IpLimited);
}

#[tokio::test]
async fn counters_keep_the_minimum_kv_ttl() {
    let store = MemoryStore::default();
    let limiter = RateLimiter::new(&store, Limits::default());
    let window_start = NOW / 60 * 60;

    limiter
        .check("1.1.1.1", &user("a"), window_start + 59)
        .await;

    assert_eq!(
        store.ttl(&format!("rl:gl:1.1.1.1:{window_start}")),
        Some(60)
    );
    assert_eq!(
        store
            .value(&format!("rl:1.1.1.1:a:{window_start}"))
            .as_deref(),
        Some("1")
    );
}

#[tokio::test]
async fn unreadable_counters_count_as_zero() {
    let store = MemoryStore::default();
    let window_start = NOW / 60 * 60;
    store.put(
        &format!("rl:1.1.1.1:a:{window_start}"),
        "garbage".to_owned(),
        60,
    );
    let limiter = RateLimiter::new(&store, limits(100, 100, 2));

    assert_eq!(
        limiter.check("1.1.1.1", &user("a"), NOW).await,
        Verdict::Allowed {
            remaining: 1,
            reset: window_start + 60
        }
    );
}
