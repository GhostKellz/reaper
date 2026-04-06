use chrono::{DateTime, Utc};
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub build_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: PathBuf,
    pub makepkg_flags: Vec<String>,
    pub parallel_downloads: usize,
    pub keep_build_dir: bool,
    pub sign_packages: bool,
    pub compression: CompressionType,
    pub build_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lz4,
}

impl Default for BuildConfig {
    fn default() -> Self {
        let cache_dir = crate::paths::CACHE_DIR.clone();

        Self {
            build_dir: cache_dir.join("build"),
            cache_dir: cache_dir.join("cache"),
            log_dir: cache_dir.join("logs"),
            makepkg_flags: vec![
                "--syncdeps".to_string(),
                "--clean".to_string(),
                "--noconfirm".to_string(),
            ],
            parallel_downloads: 5,
            keep_build_dir: false,
            sign_packages: false,
            compression: CompressionType::Zstd,
            build_timeout: 3600,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildTask {
    pub package_name: String,
    pub pkgbuild_path: PathBuf,
    pub dependencies: Vec<String>,
    pub make_dependencies: Vec<String>,
    pub artifacts: Vec<PathBuf>,
    pub status: BuildStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub log_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildStatus {
    Pending,
    FetchingSources,
    ResolvingDependencies,
    Building,
    Packaging,
    Signing,
    Completed,
    Failed(String),
    Cancelled,
}

pub struct BuildSystem {
    config: BuildConfig,
    active_builds: Arc<Mutex<HashMap<String, BuildTask>>>,
    build_cache: Arc<Mutex<HashMap<String, BuildArtifact>>>,
    multi_progress: MultiProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub package_name: String,
    pub version: String,
    pub architecture: String,
    pub file_path: PathBuf,
    pub checksum: String,
    pub size: u64,
    pub build_date: DateTime<Utc>,
    pub dependencies: Vec<String>,
}

impl BuildSystem {
    pub fn new(config: BuildConfig) -> Result<Self, Box<dyn std::error::Error>> {
        fs::create_dir_all(&config.build_dir)?;
        fs::create_dir_all(&config.cache_dir)?;
        fs::create_dir_all(&config.log_dir)?;

        let cache = Self::load_cache(&config.cache_dir)?;

        Ok(Self {
            config,
            active_builds: Arc::new(Mutex::new(HashMap::new())),
            build_cache: Arc::new(Mutex::new(cache)),
            multi_progress: MultiProgress::new(),
        })
    }

    fn load_cache(
        cache_dir: &Path,
    ) -> Result<HashMap<String, BuildArtifact>, Box<dyn std::error::Error>> {
        let cache_file = cache_dir.join("artifacts.json");
        if !cache_file.exists() {
            return Ok(HashMap::new());
        }

        let data = fs::read_to_string(cache_file)?;
        let artifacts: Vec<BuildArtifact> = serde_json::from_str(&data)?;
        let mut cache = HashMap::new();

        for artifact in artifacts {
            if artifact.file_path.exists() {
                cache.insert(artifact.package_name.clone(), artifact);
            }
        }

        Ok(cache)
    }

    pub async fn build_package(
        &self,
        task: BuildTask,
    ) -> Result<BuildArtifact, Box<dyn std::error::Error>> {
        let package_name = task.package_name.clone();

        {
            let mut builds = self.active_builds.lock().unwrap();
            builds.insert(package_name.clone(), task.clone());
        }

        let progress = self.create_progress_bar(&package_name);

        let result = self.execute_build(task, progress).await;

        {
            let mut builds = self.active_builds.lock().unwrap();
            builds.remove(&package_name);
        }

        result
    }

    async fn execute_build(
        &self,
        mut task: BuildTask,
        progress: ProgressBar,
    ) -> Result<BuildArtifact, Box<dyn std::error::Error>> {
        task.started_at = Some(Utc::now());
        task.status = BuildStatus::FetchingSources;

        progress.set_message("Fetching sources...");
        self.fetch_sources(&task).await?;

        task.status = BuildStatus::ResolvingDependencies;
        progress.set_message("Resolving dependencies...");
        self.resolve_dependencies(&task).await?;

        task.status = BuildStatus::Building;
        progress.set_message("Building package...");
        let build_output = self.run_makepkg(&task, &progress).await?;

        task.status = BuildStatus::Packaging;
        progress.set_message("Creating package...");

        let artifact = self.create_artifact(&task, build_output)?;

        if self.config.sign_packages {
            task.status = BuildStatus::Signing;
            progress.set_message("Signing package...");
            self.sign_package(&artifact)?;
        }

        task.status = BuildStatus::Completed;
        task.completed_at = Some(Utc::now());
        progress.finish_with_message(
            format!("✓ {} built successfully", task.package_name)
                .green()
                .to_string(),
        );

        {
            let mut cache = self.build_cache.lock().unwrap();
            cache.insert(task.package_name.clone(), artifact.clone());
            self.save_cache()?;
        }

        if !self.config.keep_build_dir {
            self.cleanup_build_dir(&task)?;
        }

        Ok(artifact)
    }

    async fn fetch_sources(&self, task: &BuildTask) -> Result<(), Box<dyn std::error::Error>> {
        let output = TokioCommand::new("makepkg")
            .arg("--nobuild")
            .arg("--nodeps")
            .arg("--noextract")
            .current_dir(task.pkgbuild_path.parent().unwrap())
            .output()
            .await?;

        if !output.status.success() {
            return Err(format!(
                "Failed to fetch sources: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        Ok(())
    }

    async fn resolve_dependencies(
        &self,
        task: &BuildTask,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let all_deps: Vec<String> = task
            .dependencies
            .iter()
            .chain(task.make_dependencies.iter())
            .cloned()
            .collect();

        if all_deps.is_empty() {
            return Ok(());
        }

        println!(
            "{} Installing dependencies: {}",
            "→".blue(),
            all_deps.join(", ")
        );

        let output = TokioCommand::new("pacman")
            .arg("-S")
            .arg("--needed")
            .arg("--noconfirm")
            .args(&all_deps)
            .output()
            .await?;

        if !output.status.success() {
            return Err(format!(
                "Failed to install dependencies: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        Ok(())
    }

    async fn run_makepkg(
        &self,
        task: &BuildTask,
        progress: &ProgressBar,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let log_file = self.config.log_dir.join(format!(
            "{}-{}.log",
            task.package_name,
            Utc::now().timestamp()
        ));

        let mut cmd = TokioCommand::new("makepkg");
        cmd.args(&self.config.makepkg_flags)
            .current_dir(task.pkgbuild_path.parent().unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        match &self.config.compression {
            CompressionType::Gzip => cmd.env("PKGEXT", ".pkg.tar.gz"),
            CompressionType::Bzip2 => cmd.env("PKGEXT", ".pkg.tar.bz2"),
            CompressionType::Xz => cmd.env("PKGEXT", ".pkg.tar.xz"),
            CompressionType::Zstd => cmd.env("PKGEXT", ".pkg.tar.zst"),
            CompressionType::Lz4 => cmd.env("PKGEXT", ".pkg.tar.lz4"),
        };

        let mut child = cmd.spawn()?;

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await? {
                if line.contains("Compressing") {
                    progress.set_message("Compressing package...");
                } else if line.contains("Entering fakeroot") {
                    progress.set_message("Entering fakeroot environment...");
                } else if line.contains("Stripping") {
                    progress.set_message("Stripping unneeded symbols...");
                }

                let _ = fs::write(&log_file, format!("{}\n", line));
            }
        }

        let status = child.wait().await?;

        if !status.success() {
            return Err(format!("makepkg failed. Check log: {:?}", log_file).into());
        }

        let package_file = self.find_package_file(task.pkgbuild_path.parent().unwrap())?;
        Ok(package_file)
    }

    fn find_package_file(&self, build_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let entries = fs::read_dir(build_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if (path.extension().and_then(|s| s.to_str()) == Some("zst")
                || path.extension().and_then(|s| s.to_str()) == Some("xz")
                || path.extension().and_then(|s| s.to_str()) == Some("gz"))
                && path.to_string_lossy().contains(".pkg.tar")
            {
                return Ok(path);
            }
        }

        Err("Package file not found".into())
    }

    fn create_artifact(
        &self,
        task: &BuildTask,
        package_path: PathBuf,
    ) -> Result<BuildArtifact, Box<dyn std::error::Error>> {
        let metadata = fs::metadata(&package_path)?;
        let checksum = self.calculate_checksum(&package_path)?;

        let filename = package_path
            .file_name()
            .ok_or("Invalid package filename")?
            .to_string_lossy();

        let parts: Vec<&str> = filename.split('-').collect();
        let version = if parts.len() >= 2 {
            parts[parts.len() - 2].to_string()
        } else {
            "unknown".to_string()
        };

        let arch = if filename.contains("x86_64") {
            "x86_64".to_string()
        } else if filename.contains("any") {
            "any".to_string()
        } else {
            "unknown".to_string()
        };

        let dest_path = self
            .config
            .cache_dir
            .join(package_path.file_name().unwrap());
        fs::copy(&package_path, &dest_path)?;

        Ok(BuildArtifact {
            package_name: task.package_name.clone(),
            version,
            architecture: arch,
            file_path: dest_path,
            checksum,
            size: metadata.len(),
            build_date: Utc::now(),
            dependencies: task.dependencies.clone(),
        })
    }

    fn calculate_checksum(&self, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let mut file = fs::File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let hash = Sha256::digest(&buffer);
        Ok(hex::encode(hash))
    }

    fn sign_package(&self, artifact: &BuildArtifact) -> Result<(), Box<dyn std::error::Error>> {
        let output = Command::new("gpg")
            .arg("--detach-sign")
            .arg("--no-armor")
            .arg(&artifact.file_path)
            .output()?;

        if !output.status.success() {
            return Err(format!(
                "Failed to sign package: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        Ok(())
    }

    fn cleanup_build_dir(&self, task: &BuildTask) -> Result<(), Box<dyn std::error::Error>> {
        let build_dir = task.pkgbuild_path.parent().unwrap();

        for entry in fs::read_dir(build_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir()
                && (path.file_name().unwrap() == "src" || path.file_name().unwrap() == "pkg")
            {
                fs::remove_dir_all(path)?;
            }
        }

        Ok(())
    }

    fn create_progress_bar(&self, package_name: &str) -> ProgressBar {
        let pb = self.multi_progress.add(ProgressBar::new(100));
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "{{spinner:.green}} [{{bar:40.cyan/blue}}] {}: {{msg}}",
                    package_name.bold()
                ))
                .unwrap()
                .progress_chars("#>-"),
        );
        pb
    }

    fn save_cache(&self) -> Result<(), Box<dyn std::error::Error>> {
        let cache = self.build_cache.lock().unwrap();
        let artifacts: Vec<BuildArtifact> = cache.values().cloned().collect();

        let cache_file = self.config.cache_dir.join("artifacts.json");
        let data = serde_json::to_string_pretty(&artifacts)?;
        fs::write(cache_file, data)?;

        Ok(())
    }

    pub fn get_cached_artifact(&self, package_name: &str) -> Option<BuildArtifact> {
        let cache = self.build_cache.lock().unwrap();
        cache.get(package_name).cloned()
    }

    pub fn clean_cache(
        &self,
        older_than_days: Option<u64>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut cache = self.build_cache.lock().unwrap();
        let cutoff = if let Some(days) = older_than_days {
            Utc::now() - chrono::Duration::days(days as i64)
        } else {
            Utc::now()
        };

        let mut removed = 0;
        cache.retain(|_, artifact| {
            if artifact.build_date < cutoff {
                if artifact.file_path.exists() {
                    let _ = fs::remove_file(&artifact.file_path);
                }
                removed += 1;
                false
            } else {
                true
            }
        });

        self.save_cache()?;
        Ok(removed)
    }

    pub fn get_active_builds(&self) -> Vec<BuildTask> {
        let builds = self.active_builds.lock().unwrap();
        builds.values().cloned().collect()
    }

    pub fn cancel_build(&self, package_name: &str) -> bool {
        let mut builds = self.active_builds.lock().unwrap();
        if let Some(task) = builds.get_mut(package_name) {
            task.status = BuildStatus::Cancelled;
            true
        } else {
            false
        }
    }
}

pub async fn parallel_build(
    packages: Vec<BuildTask>,
    config: BuildConfig,
) -> Result<Vec<BuildArtifact>, Box<dyn std::error::Error>> {
    let system = BuildSystem::new(config)?;
    let mut handles = Vec::new();
    let artifacts = Arc::new(Mutex::new(Vec::new()));

    for task in packages {
        let system_clone = system.clone();
        let artifacts_clone = artifacts.clone();

        let handle = tokio::spawn(async move {
            match system_clone.build_package(task).await {
                Ok(artifact) => {
                    let mut arts = artifacts_clone.lock().unwrap();
                    arts.push(artifact);
                }
                Err(e) => eprintln!("Build failed: {}", e),
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    let final_artifacts = artifacts.lock().unwrap().clone();
    Ok(final_artifacts)
}

impl Clone for BuildSystem {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            active_builds: self.active_builds.clone(),
            build_cache: self.build_cache.clone(),
            multi_progress: MultiProgress::new(),
        }
    }
}
