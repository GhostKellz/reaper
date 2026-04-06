use chrono::{DateTime, Duration, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: String,
    pub title: String,
    pub content: String,
    pub author: String,
    pub published: DateTime<Utc>,
    pub url: String,
    pub tags: Vec<String>,
    pub importance: NewsImportance,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NewsImportance {
    Critical,
    High,
    Medium,
    Low,
}

impl std::fmt::Display for NewsImportance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewsImportance::Critical => write!(f, "{}", "CRITICAL".red().bold()),
            NewsImportance::High => write!(f, "{}", "HIGH".yellow().bold()),
            NewsImportance::Medium => write!(f, "{}", "MEDIUM".blue()),
            NewsImportance::Low => write!(f, "{}", "LOW".white()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewsCache {
    pub items: Vec<NewsItem>,
    pub last_updated: DateTime<Utc>,
    pub read_items: Vec<String>,
}

impl Default for NewsCache {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            last_updated: Utc::now() - Duration::days(30),
            read_items: Vec::new(),
        }
    }
}

pub struct NewsManager {
    cache_path: PathBuf,
    cache: NewsCache,
    rss_feeds: Vec<String>,
    #[allow(dead_code)] // Reserved for future keyword-based filtering
    keywords: Vec<String>,
}

impl NewsManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config_dir = crate::paths::CONFIG_DIR.clone();

        fs::create_dir_all(&config_dir)?;
        let cache_path = config_dir.join("news_cache.json");

        let cache = if cache_path.exists() {
            let data = fs::read_to_string(&cache_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            NewsCache::default()
        };

        Ok(Self {
            cache_path,
            cache,
            rss_feeds: vec![
                "https://archlinux.org/feeds/news/".to_string(),
                "https://security.archlinux.org/rss".to_string(),
            ],
            keywords: vec![
                "breaking".to_string(),
                "critical".to_string(),
                "security".to_string(),
                "vulnerability".to_string(),
                "manual intervention".to_string(),
                "upgrade".to_string(),
            ],
        })
    }

    pub async fn fetch_news(&mut self) -> Result<Vec<NewsItem>, Box<dyn std::error::Error>> {
        let mut all_news = Vec::new();

        for feed_url in &self.rss_feeds {
            match self.fetch_rss_feed(feed_url).await {
                Ok(items) => all_news.extend(items),
                Err(e) => eprintln!("Failed to fetch {}: {}", feed_url, e),
            }
        }

        all_news.sort_by(|a, b| b.published.cmp(&a.published));

        for item in &mut all_news {
            if self.cache.read_items.contains(&item.id) {
                item.read = true;
            }
            item.importance = self.determine_importance(item);
        }

        self.cache.items = all_news.clone();
        self.cache.last_updated = Utc::now();
        self.save_cache()?;

        Ok(all_news)
    }

    async fn fetch_rss_feed(&self, url: &str) -> Result<Vec<NewsItem>, Box<dyn std::error::Error>> {
        let response = reqwest::get(url).await?;
        let content = response.text().await?;

        let mut items = Vec::new();
        let doc = roxmltree::Document::parse(&content)?;

        for node in doc.descendants() {
            if (node.tag_name().name() == "item" || node.tag_name().name() == "entry")
                && let Some(item) = self.parse_rss_item(node, url)
            {
                items.push(item);
            }
        }

        Ok(items)
    }

    fn parse_rss_item(&self, node: roxmltree::Node, feed_url: &str) -> Option<NewsItem> {
        let title = node
            .descendants()
            .find(|n| n.tag_name().name() == "title")
            .and_then(|n| n.text())
            .unwrap_or("Untitled")
            .to_string();

        let content = node
            .descendants()
            .find(|n| n.tag_name().name() == "description" || n.tag_name().name() == "summary")
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_string();

        let author = node
            .descendants()
            .find(|n| n.tag_name().name() == "author" || n.tag_name().name() == "dc:creator")
            .and_then(|n| n.text())
            .unwrap_or("Unknown")
            .to_string();

        let url = node
            .descendants()
            .find(|n| n.tag_name().name() == "link")
            .and_then(|n| n.text())
            .unwrap_or(feed_url)
            .to_string();

        let published_str = node
            .descendants()
            .find(|n| n.tag_name().name() == "pubDate" || n.tag_name().name() == "published")
            .and_then(|n| n.text())
            .unwrap_or("");

        let published = DateTime::parse_from_rfc3339(published_str)
            .or_else(|_| DateTime::parse_from_rfc2822(published_str))
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let mut tags = Vec::new();
        for category in node
            .descendants()
            .filter(|n| n.tag_name().name() == "category")
        {
            if let Some(tag) = category.text() {
                tags.push(tag.to_string());
            }
        }

        let id = format!(
            "{}-{}",
            published.timestamp(),
            title.chars().take(20).collect::<String>().replace(' ', "-")
        );

        Some(NewsItem {
            id,
            title,
            content,
            author,
            published,
            url,
            tags,
            importance: NewsImportance::Low,
            read: false,
        })
    }

    fn determine_importance(&self, item: &NewsItem) -> NewsImportance {
        let text = format!(
            "{} {}",
            item.title.to_lowercase(),
            item.content.to_lowercase()
        );

        if text.contains("critical") || text.contains("manual intervention required") {
            return NewsImportance::Critical;
        }

        if text.contains("security") || text.contains("vulnerability") || text.contains("urgent") {
            return NewsImportance::High;
        }

        if text.contains("important") || text.contains("breaking") || text.contains("upgrade") {
            return NewsImportance::Medium;
        }

        NewsImportance::Low
    }

    pub fn get_unread_news(&self) -> Vec<&NewsItem> {
        self.cache.items.iter().filter(|item| !item.read).collect()
    }

    pub fn get_critical_news(&self) -> Vec<&NewsItem> {
        self.cache
            .items
            .iter()
            .filter(|item| item.importance == NewsImportance::Critical && !item.read)
            .collect()
    }

    pub fn mark_as_read(&mut self, item_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !self.cache.read_items.contains(&item_id.to_string()) {
            self.cache.read_items.push(item_id.to_string());
        }

        if let Some(item) = self.cache.items.iter_mut().find(|i| i.id == item_id) {
            item.read = true;
        }

        self.save_cache()?;
        Ok(())
    }

    pub fn mark_all_as_read(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for item in &mut self.cache.items {
            item.read = true;
            if !self.cache.read_items.contains(&item.id) {
                self.cache.read_items.push(item.id.clone());
            }
        }
        self.save_cache()?;
        Ok(())
    }

    pub fn display_news(&self, limit: Option<usize>) {
        let items: Vec<&NewsItem> = if let Some(limit) = limit {
            self.cache.items.iter().take(limit).collect()
        } else {
            self.cache.items.iter().collect()
        };

        if items.is_empty() {
            println!("{}", "No news items available.".yellow());
            return;
        }

        println!("{}", "=== Arch Linux News ===".bold().blue());
        println!(
            "Last updated: {}",
            self.cache.last_updated.format("%Y-%m-%d %H:%M:%S")
        );
        println!();

        for (idx, item) in items.iter().enumerate() {
            let read_indicator = if item.read {
                "✓".green()
            } else {
                "●".red().bold()
            };

            println!(
                "{} {} [{}] {}",
                read_indicator,
                format!("#{}", idx + 1).white().dimmed(),
                item.importance,
                item.title.bold()
            );

            println!(
                "  {} | {}",
                item.author.cyan(),
                item.published
                    .format("%Y-%m-%d")
                    .to_string()
                    .white()
                    .dimmed()
            );

            if !item.tags.is_empty() {
                println!("  Tags: {}", item.tags.join(", ").italic());
            }

            let preview = if item.content.len() > 150 {
                format!("{}...", &item.content[..150])
            } else {
                item.content.clone()
            };
            println!("  {}", preview.white().dimmed());
            println!("  URL: {}", item.url.blue().underline());
            println!();
        }
    }

    pub fn display_summary(&self) {
        let unread_count = self.get_unread_news().len();
        let critical_count = self.get_critical_news().len();

        if critical_count > 0 {
            println!(
                "{} {} critical news items require your attention!",
                "⚠".red().bold(),
                critical_count.to_string().red().bold()
            );
        }

        if unread_count > 0 {
            println!(
                "{} You have {} unread news items.",
                "ℹ".blue(),
                unread_count.to_string().yellow()
            );
        } else {
            println!("{} All news items have been read.", "✓".green());
        }
    }

    fn save_cache(&self) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_string_pretty(&self.cache)?;
        fs::write(&self.cache_path, data)?;
        Ok(())
    }

    pub fn should_refresh(&self) -> bool {
        Utc::now() - self.cache.last_updated > Duration::hours(6)
    }
}

pub async fn check_news_on_startup() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = NewsManager::new()?;

    if manager.should_refresh() {
        println!("{} Checking for Arch Linux news...", "→".blue());
        manager.fetch_news().await?;
    }

    let critical = manager.get_critical_news();
    if !critical.is_empty() {
        println!();
        println!("{}", "═══════════════════════════════════════".red().bold());
        println!(
            "{} CRITICAL NEWS - Manual intervention may be required!",
            "⚠".red().bold()
        );
        println!("{}", "═══════════════════════════════════════".red().bold());

        for item in critical {
            println!();
            println!("{}", item.title.red().bold());
            println!("{}", item.published.format("%Y-%m-%d"));
            println!("{}", item.url.blue().underline());
            println!();
        }

        println!(
            "{}",
            "Please review these news items before proceeding.".yellow()
        );
        println!("{}", "═══════════════════════════════════════".red().bold());
        println!();
    } else {
        manager.display_summary();
    }

    Ok(())
}
