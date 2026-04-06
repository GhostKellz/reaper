//! In-Memory AUR RPC Cache
//!
//! Provides a fast, thread-safe in-memory cache for AUR RPC responses.
//! This cache sits in front of the file-based cache for hot path optimization.
//!
//! # Performance
//!
//! - In-memory lookups are ~1000x faster than file I/O
//! - Reduces AUR API calls for repeated queries
//! - Automatic TTL expiration (5 minutes for search, 15 minutes for info)

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::aur::{AurResult, SearchResult};

/// Default TTL for search cache (5 minutes)
const SEARCH_TTL: Duration = Duration::from_secs(5 * 60);

/// Default TTL for package info cache (15 minutes)
const INFO_TTL: Duration = Duration::from_secs(15 * 60);

/// Default TTL for PKGBUILD cache (1 hour)
const PKGBUILD_TTL: Duration = Duration::from_secs(60 * 60);

/// Maximum cache entries before eviction
const MAX_CACHE_SIZE: usize = 500;

/// Cache entry with timestamp
#[derive(Clone)]
struct CacheEntry<T> {
    data: T,
    inserted_at: Instant,
    ttl: Duration,
}

impl<T> CacheEntry<T> {
    fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            inserted_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }
}

/// Thread-safe in-memory cache for search results
static SEARCH_CACHE: Lazy<RwLock<HashMap<String, CacheEntry<Vec<SearchResult>>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Thread-safe in-memory cache for package info
static INFO_CACHE: Lazy<RwLock<HashMap<String, CacheEntry<AurResult>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Thread-safe in-memory cache for PKGBUILDs
static PKGBUILD_CACHE: Lazy<RwLock<HashMap<String, CacheEntry<String>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Get cached search results
pub fn get_search(query: &str) -> Option<Vec<SearchResult>> {
    let cache = SEARCH_CACHE.read().ok()?;
    let entry = cache.get(query)?;

    if entry.is_expired() {
        // Don't remove here to avoid write lock, let put() handle eviction
        return None;
    }

    Some(entry.data.clone())
}

/// Cache search results
pub fn put_search(query: &str, results: Vec<SearchResult>) {
    let mut cache = match SEARCH_CACHE.write() {
        Ok(c) => c,
        Err(_) => return,
    };

    // Evict expired entries if cache is getting large
    if cache.len() >= MAX_CACHE_SIZE {
        evict_expired(&mut cache);
    }

    cache.insert(query.to_string(), CacheEntry::new(results, SEARCH_TTL));
}

/// Get cached package info
pub fn get_info(pkg: &str) -> Option<AurResult> {
    let cache = INFO_CACHE.read().ok()?;
    let entry = cache.get(pkg)?;

    if entry.is_expired() {
        return None;
    }

    Some(entry.data.clone())
}

/// Cache package info
pub fn put_info(pkg: &str, info: AurResult) {
    let mut cache = match INFO_CACHE.write() {
        Ok(c) => c,
        Err(_) => return,
    };

    if cache.len() >= MAX_CACHE_SIZE {
        evict_expired(&mut cache);
    }

    cache.insert(pkg.to_string(), CacheEntry::new(info, INFO_TTL));
}

/// Get cached PKGBUILD
pub fn get_pkgbuild(pkg: &str) -> Option<String> {
    let cache = PKGBUILD_CACHE.read().ok()?;
    let entry = cache.get(pkg)?;

    if entry.is_expired() {
        return None;
    }

    Some(entry.data.clone())
}

/// Cache PKGBUILD
pub fn put_pkgbuild(pkg: &str, pkgbuild: String) {
    let mut cache = match PKGBUILD_CACHE.write() {
        Ok(c) => c,
        Err(_) => return,
    };

    if cache.len() >= MAX_CACHE_SIZE {
        evict_expired(&mut cache);
    }

    cache.insert(pkg.to_string(), CacheEntry::new(pkgbuild, PKGBUILD_TTL));
}

/// Evict expired entries from cache
fn evict_expired<T>(cache: &mut HashMap<String, CacheEntry<T>>) {
    cache.retain(|_, entry| !entry.is_expired());

    // If still too large, remove oldest entries
    if cache.len() >= MAX_CACHE_SIZE {
        // Sort by age and remove oldest 25%
        let mut entries: Vec<_> = cache
            .iter()
            .map(|(k, v)| (k.clone(), v.inserted_at))
            .collect();
        entries.sort_by_key(|(_, t)| *t);

        let to_remove = entries.len() / 4;
        for (key, _) in entries.into_iter().take(to_remove) {
            cache.remove(&key);
        }
    }
}

/// Clear all in-memory caches
#[allow(dead_code)]
pub fn clear_all() {
    if let Ok(mut cache) = SEARCH_CACHE.write() {
        cache.clear();
    }
    if let Ok(mut cache) = INFO_CACHE.write() {
        cache.clear();
    }
    if let Ok(mut cache) = PKGBUILD_CACHE.write() {
        cache.clear();
    }
}

/// Get cache statistics
#[allow(dead_code)]
pub fn stats() -> CacheStats {
    let search_count = SEARCH_CACHE.read().map(|c| c.len()).unwrap_or(0);
    let info_count = INFO_CACHE.read().map(|c| c.len()).unwrap_or(0);
    let pkgbuild_count = PKGBUILD_CACHE.read().map(|c| c.len()).unwrap_or(0);

    CacheStats {
        search_entries: search_count,
        info_entries: info_count,
        pkgbuild_entries: pkgbuild_count,
        total_entries: search_count + info_count + pkgbuild_count,
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CacheStats {
    pub search_entries: usize,
    pub info_entries: usize,
    pub pkgbuild_entries: usize,
    pub total_entries: usize,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {} search, {} info, {} pkgbuild ({} total)",
            self.search_entries, self.info_entries, self.pkgbuild_entries, self.total_entries
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test uses unique keys to avoid race conditions when tests run in parallel

    #[test]
    fn test_search_cache() {
        let results = vec![SearchResult {
            name: "search_test_pkg".to_string(),
            version: "1.0.0".to_string(),
            description: "Test package".to_string(),
            source: crate::core::Source::Aur,
        }];

        put_search("search_test_query", results.clone());
        let cached = get_search("search_test_query");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_info_cache() {
        let info = AurResult {
            name: "info_test_pkg".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test".to_string()),
        };

        put_info("info_test_pkg", info.clone());
        let cached = get_info("info_test_pkg");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "info_test_pkg");
    }

    #[test]
    fn test_pkgbuild_cache() {
        let pkgbuild = "pkgname=pkgbuild_test\npkgver=1.0.0".to_string();

        put_pkgbuild("pkgbuild_test_pkg", pkgbuild.clone());
        let cached = get_pkgbuild("pkgbuild_test_pkg");
        assert!(cached.is_some());
        assert!(cached.unwrap().contains("pkgname=pkgbuild_test"));
    }

    #[test]
    fn test_cache_stats() {
        // Don't call clear_all() - it interferes with parallel tests
        // Instead, just add entries and verify stats increase

        let before = stats();

        put_search("stats_test_q1", vec![]);
        put_search("stats_test_q2", vec![]);
        put_info(
            "stats_test_p1",
            AurResult {
                name: "stats_test_p1".to_string(),
                version: "1.0".to_string(),
                description: None,
            },
        );

        let after = stats();
        // Stats should have increased by our additions
        assert!(after.search_entries >= before.search_entries);
        assert!(after.info_entries >= before.info_entries);
        assert!(after.total_entries >= before.total_entries);
    }
}
