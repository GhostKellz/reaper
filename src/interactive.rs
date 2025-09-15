use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRating {
    pub package: String,
    pub aur_votes: u32,
    pub aur_popularity: f64,
    pub user_rating: Option<u8>, // 1-5 stars
    pub community_rating: f64,
    pub reviews: Vec<PackageReview>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReview {
    pub user: String,
    pub rating: u8,
    pub comment: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub helpful_votes: u32,
}

pub struct InteractiveManager {
    ratings_cache: HashMap<String, PackageRating>,
    cache_dir: PathBuf,
}

impl InteractiveManager {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap/ratings");
        let _ = fs::create_dir_all(&cache_dir);

        Self {
            ratings_cache: HashMap::new(),
            cache_dir,
        }
    }

    /// Interactive confirmation prompt
    pub fn confirm_action(message: &str, default: bool) -> bool {
        let _default_char = if default { 'Y' } else { 'N' };
        let prompt = format!("{} [{}]: ", message, if default { "Y/n" } else { "y/N" });

        print!("{}", prompt);
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let trimmed = input.trim().to_lowercase();
            match trimmed.as_str() {
                "y" | "yes" => true,
                "n" | "no" => false,
                "" => default,
                _ => Self::confirm_action(message, default), // Use Self instead of self
            }
        } else {
            default
        }
    }

    /// Interactive package removal confirmation
    pub fn confirm_removal(&self, packages: &[String]) -> bool {
        println!("\n🗑️  The following packages will be REMOVED:");
        for pkg in packages {
            println!("  - {}", pkg);
        }
        println!();

        InteractiveManager::confirm_action("Do you want to continue?", false)
    }

    /// Interactive PKGBUILD editing
    pub fn confirm_pkgbuild_edit(&self, package: &str) -> bool {
        println!(
            "\n📝 PKGBUILD for {} is about to be opened for editing.",
            package
        );
        println!("⚠️  Only edit if you understand the implications!");

        InteractiveManager::confirm_action("Do you want to edit the PKGBUILD?", false)
    }

    /// Get package rating with AUR integration
    pub async fn get_package_rating(&mut self, package: &str) -> Result<PackageRating> {
        // Check cache first
        if let Some(rating) = self.ratings_cache.get(package) {
            return Ok(rating.clone());
        }

        // Fetch from AUR API
        self.fetch_aur_info(package).await?;

        let rating = PackageRating {
            package: package.to_string(),
            aur_votes: 0,
            aur_popularity: 0.0,
            user_rating: None,
            community_rating: self.calculate_community_rating(&()),
            reviews: Vec::new(),
            last_updated: chrono::Utc::now(),
        };

        // Cache it
        self.ratings_cache
            .insert(package.to_string(), rating.clone());
        self.save_rating_to_cache(&rating)?;

        Ok(rating)
    }

    async fn fetch_aur_info(&self, package: &str) -> Result<()> {
        let url = format!(
            "https://aur.archlinux.org/rpc/?v=5&type=info&arg[]={}",
            package
        );
        let resp = reqwest::get(&url).await?;
        let _ = resp.text().await?;
        Ok(())
    }

    fn calculate_community_rating(&self, _aur_info: &()) -> f64 {
        // Simplified stub
        3.5
    }

    /// Display rating with stars
    pub fn display_rating(&self, rating: &PackageRating) -> String {
        let stars = self.rating_to_stars(rating.community_rating);
        format!(
            "{} {:.1}/5.0 ({} votes)",
            stars, rating.community_rating, rating.aur_votes
        )
    }

    fn rating_to_stars(&self, rating: f64) -> String {
        let full_stars = rating.floor() as usize;
        let half_star = (rating - rating.floor()) >= 0.5;
        let empty_stars = 5 - full_stars - if half_star { 1 } else { 0 };
        "⭐".repeat(full_stars) + if half_star { "⭐" } else { "" } + &"☆".repeat(empty_stars)
    }

    /// User rating submission
    pub fn submit_user_rating(
        &mut self,
        package: &str,
        rating: u8,
        comment: Option<String>,
    ) -> Result<()> {
        if rating > 5 {
            return Err(anyhow::anyhow!("Rating must be between 1 and 5"));
        }

        let mut pkg_rating =
            self.ratings_cache
                .get(package)
                .cloned()
                .unwrap_or_else(|| PackageRating {
                    package: package.to_string(),
                    aur_votes: 0,
                    aur_popularity: 0.0,
                    user_rating: None,
                    community_rating: 0.0,
                    reviews: Vec::new(),
                    last_updated: chrono::Utc::now(),
                });

        pkg_rating.user_rating = Some(rating);

        if let Some(comment) = comment {
            pkg_rating.reviews.push(PackageReview {
                user: "local_user".to_string(),
                rating,
                comment,
                date: chrono::Utc::now(),
                helpful_votes: 0,
            });
        }

        self.ratings_cache
            .insert(package.to_string(), pkg_rating.clone());
        self.save_rating_to_cache(&pkg_rating)?;

        println!("✅ Rating submitted for {}: {}/5", package, rating);
        Ok(())
    }

    fn save_rating_to_cache(&self, rating: &PackageRating) -> Result<()> {
        let cache_file = self.cache_dir.join(format!("{}.json", rating.package));
        let content = serde_json::to_string_pretty(rating)?;
        fs::write(cache_file, content)?;
        Ok(())
    }

    /// Interactive package selection
    #[allow(dead_code)]
    pub fn select_from_list(&self, items: &[String], prompt: &str) -> Option<usize> {
        println!("\n{}", prompt);
        for (i, item) in items.iter().enumerate() {
            println!("  {}: {}", i + 1, item);
        }

        print!("\nEnter your choice (1-{}): ", items.len());
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            if let Ok(choice) = input.trim().parse::<usize>() {
                if choice > 0 && choice <= items.len() {
                    return Some(choice - 1);
                }
            }
        }

        println!("Invalid selection.");
        None
    }
}

impl Default for InteractiveManager {
    fn default() -> Self {
        Self::new()
    }
}
