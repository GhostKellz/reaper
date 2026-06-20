use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetrics {
    pub package: String,
    pub version: String,
    pub source: crate::core::Source,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration: Option<i64>, // seconds
    pub success: Option<bool>,
    pub parallel_jobs: u32,
    pub cpu_usage_peak: Option<f32>,
    pub memory_usage_peak: Option<u64>, // MB
    pub disk_io: Option<u64>,           // MB
    pub cache_hit: bool,
    pub download_size: Option<u64>, // MB
    pub error_type: Option<String>,
    pub profile_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub total_builds: u32,
    pub success_rate: f32,
    pub average_duration: f64, // minutes
    pub fastest_build: Option<BuildMetrics>,
    pub slowest_build: Option<BuildMetrics>,
    pub most_failed_package: Option<String>,
    pub cache_hit_rate: f32,
    pub total_data_downloaded: u64, // MB
    pub profile_performance: HashMap<String, ProfilePerformance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePerformance {
    pub builds: u32,
    pub success_rate: f32,
    pub average_duration: f64,
    pub parallel_efficiency: f32,
}

pub struct PerformanceAnalyzer {
    metrics_dir: PathBuf,
    current_builds: HashMap<String, (Instant, BuildMetrics)>,
}

impl PerformanceAnalyzer {
    pub fn new() -> Self {
        let metrics_dir = crate::paths::metrics_dir();
        let _ = fs::create_dir_all(&metrics_dir);

        Self {
            metrics_dir,
            current_builds: HashMap::new(),
        }
    }

    /// Start tracking a build
    pub fn start_build(
        &mut self,
        pkg: &str,
        version: &str,
        source: &crate::core::Source,
        profile: &str,
        parallel_jobs: u32,
    ) -> String {
        let build_id = format!("{}-{}", pkg, Utc::now().format("%Y%m%d%H%M%S"));
        let metrics = BuildMetrics {
            package: pkg.to_string(),
            version: version.to_string(),
            source: source.clone(),
            start_time: Utc::now(),
            end_time: None,
            duration: None,
            success: None,
            parallel_jobs,
            cpu_usage_peak: None,
            memory_usage_peak: None,
            disk_io: None,
            cache_hit: false,
            download_size: None,
            error_type: None,
            profile_used: profile.to_string(),
        };

        self.current_builds
            .insert(build_id.clone(), (Instant::now(), metrics));

        build_id
    }

    /// End tracking a build
    pub fn end_build(
        &mut self,
        build_id: &str,
        success: bool,
        error_type: Option<String>,
    ) -> Result<()> {
        if let Some((start_instant, mut metrics)) = self.current_builds.remove(build_id) {
            let end_time = Utc::now();
            let duration = end_time.signed_duration_since(metrics.start_time);

            metrics.end_time = Some(end_time);
            metrics.duration = Some(duration.num_seconds());
            metrics.success = Some(success);
            metrics.error_type = error_type;

            // Save metrics
            self.save_metrics(&metrics)?;

            // Print performance summary
            self.print_build_summary(&metrics, start_instant.elapsed());
        }

        Ok(())
    }

    /// Generate comprehensive performance report
    pub fn generate_report(&self, days: Option<u32>) -> Result<PerformanceReport> {
        let cutoff_date = days.map(|d| Utc::now() - Duration::days(d as i64));
        let metrics = self.load_metrics_since(cutoff_date)?;

        if metrics.is_empty() {
            return Ok(PerformanceReport {
                total_builds: 0,
                success_rate: 0.0,
                average_duration: 0.0,
                fastest_build: None,
                slowest_build: None,
                most_failed_package: None,
                cache_hit_rate: 0.0,
                total_data_downloaded: 0,
                profile_performance: HashMap::new(),
            });
        }

        let total_builds = metrics.len() as u32;
        let successful_builds = metrics.iter().filter(|m| m.success == Some(true)).count();
        let success_rate = (successful_builds as f32 / total_builds as f32) * 100.0;

        // Average duration (only successful builds)
        let successful_durations: Vec<i64> = metrics
            .iter()
            .filter_map(|m| {
                if m.success == Some(true) {
                    m.duration
                } else {
                    None
                }
            })
            .collect();
        let average_duration = if !successful_durations.is_empty() {
            successful_durations.iter().sum::<i64>() as f64
                / successful_durations.len() as f64
                / 60.0
        } else {
            0.0
        };

        // Fastest and slowest builds
        let fastest_build = metrics
            .iter()
            .filter(|m| m.success == Some(true) && m.duration.is_some())
            .min_by_key(|m| m.duration.unwrap())
            .cloned();

        let slowest_build = metrics
            .iter()
            .filter(|m| m.success == Some(true) && m.duration.is_some())
            .max_by_key(|m| m.duration.unwrap())
            .cloned();

        // Most failed package
        let mut failure_counts: HashMap<String, u32> = HashMap::new();
        for metric in &metrics {
            if metric.success == Some(false) {
                *failure_counts.entry(metric.package.clone()).or_insert(0) += 1;
            }
        }
        let most_failed_package = failure_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(pkg, _)| pkg);

        // Cache hit rate
        let cache_hits = metrics.iter().filter(|m| m.cache_hit).count();
        let cache_hit_rate = (cache_hits as f32 / total_builds as f32) * 100.0;

        // Total data downloaded
        let total_data_downloaded = metrics.iter().filter_map(|m| m.download_size).sum();

        // Profile performance
        let mut profile_stats: HashMap<String, Vec<&BuildMetrics>> = HashMap::new();
        for metric in &metrics {
            profile_stats
                .entry(metric.profile_used.clone())
                .or_default()
                .push(metric);
        }

        let profile_performance = profile_stats
            .into_iter()
            .map(|(profile, metrics)| {
                let builds = metrics.len() as u32;
                let successful = metrics.iter().filter(|m| m.success == Some(true)).count();
                let success_rate = (successful as f32 / builds as f32) * 100.0;

                let avg_duration = if successful > 0 {
                    let sum: i64 = metrics
                        .iter()
                        .filter_map(|m| {
                            if m.success == Some(true) {
                                m.duration
                            } else {
                                None
                            }
                        })
                        .sum();
                    sum as f64 / successful as f64 / 60.0
                } else {
                    0.0
                };

                // Calculate parallel efficiency (builds per minute per job)
                let parallel_efficiency = if avg_duration > 0.0 {
                    let avg_jobs: f32 = metrics.iter().map(|m| m.parallel_jobs as f32).sum::<f32>()
                        / metrics.len() as f32;
                    1.0 / (avg_duration as f32 * avg_jobs)
                } else {
                    0.0
                };

                (
                    profile,
                    ProfilePerformance {
                        builds,
                        success_rate,
                        average_duration: avg_duration,
                        parallel_efficiency,
                    },
                )
            })
            .collect();

        Ok(PerformanceReport {
            total_builds,
            success_rate,
            average_duration,
            fastest_build,
            slowest_build,
            most_failed_package,
            cache_hit_rate,
            total_data_downloaded,
            profile_performance,
        })
    }

    /// Print detailed performance report
    pub fn print_performance_report(&self, days: Option<u32>) -> Result<()> {
        let report = self.generate_report(days)?;

        println!("\n📊 Reaper Performance Report");
        if let Some(d) = days {
            println!("📅 Period: Last {} days", d);
        } else {
            println!("📅 Period: All time");
        }
        println!("{}", "=".repeat(60));

        println!("📦 Total Builds: {}", report.total_builds);
        println!("✅ Success Rate: {:.1}%", report.success_rate);
        println!(
            "⏱️  Average Duration: {:.1} minutes",
            report.average_duration
        );
        println!("🎯 Cache Hit Rate: {:.1}%", report.cache_hit_rate);
        println!("📥 Data Downloaded: {:.1} MB", report.total_data_downloaded);

        if let Some(fastest) = &report.fastest_build {
            println!(
                "🚀 Fastest Build: {} in {:.1}s",
                fastest.package,
                fastest.duration.unwrap_or(0)
            );
        }

        if let Some(slowest) = &report.slowest_build {
            println!(
                "🐌 Slowest Build: {} in {:.1}m",
                slowest.package,
                slowest.duration.unwrap_or(0) as f64 / 60.0
            );
        }

        if let Some(failed_pkg) = &report.most_failed_package {
            println!("❌ Most Failed: {}", failed_pkg);
        }

        // Profile performance
        if !report.profile_performance.is_empty() {
            println!("\n👤 Profile Performance:");
            for (profile, perf) in &report.profile_performance {
                println!(
                    "  {} - {:.1}% success, {:.1}m avg, efficiency: {:.3}",
                    profile, perf.success_rate, perf.average_duration, perf.parallel_efficiency
                );
            }
        }

        Ok(())
    }

    fn print_build_summary(&self, metrics: &BuildMetrics, elapsed: std::time::Duration) {
        let success_icon = if metrics.success == Some(true) {
            "✅"
        } else {
            "❌"
        };
        let duration_text = format!("{:.1}s", elapsed.as_secs_f64());

        println!(
            "\n{} Build completed: {} v{}",
            success_icon, metrics.package, metrics.version
        );
        println!("⏱️  Duration: {}", duration_text);
        println!("🔧 Profile: {}", metrics.profile_used);
        println!("⚡ Parallel Jobs: {}", metrics.parallel_jobs);

        if let Some(cpu) = metrics.cpu_usage_peak {
            println!("💻 Peak CPU: {:.1}%", cpu);
        }

        if let Some(memory) = metrics.memory_usage_peak {
            println!("🧠 Peak Memory: {} MB", memory);
        }
    }

    fn save_metrics(&self, metrics: &BuildMetrics) -> Result<()> {
        let date = metrics.start_time.format("%Y-%m-%d").to_string();
        let metrics_file = self.metrics_dir.join(format!("{}.jsonl", date));

        // Append to daily metrics file
        let json_line = serde_json::to_string(metrics)?;
        let content = if metrics_file.exists() {
            format!(
                "{}\n{}\n",
                fs::read_to_string(&metrics_file)?.trim(),
                json_line
            )
        } else {
            format!("{}\n", json_line)
        };

        fs::write(metrics_file, content)?;
        Ok(())
    }

    fn load_metrics_since(&self, cutoff_date: Option<DateTime<Utc>>) -> Result<Vec<BuildMetrics>> {
        let mut all_metrics = Vec::new();

        for entry in fs::read_dir(&self.metrics_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "jsonl") {
                let content = fs::read_to_string(&path)?;
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }

                    if let Ok(metrics) = serde_json::from_str::<BuildMetrics>(line) {
                        if let Some(cutoff) = cutoff_date {
                            if metrics.start_time >= cutoff {
                                all_metrics.push(metrics);
                            }
                        } else {
                            all_metrics.push(metrics);
                        }
                    }
                }
            }
        }

        Ok(all_metrics)
    }
}

impl Default for PerformanceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
