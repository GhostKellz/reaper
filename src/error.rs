use colored::Colorize;
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
    Warning,
    Info,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Critical => write!(f, "{}", "CRITICAL".red().bold()),
            ErrorSeverity::High => write!(f, "{}", "HIGH".red()),
            ErrorSeverity::Medium => write!(f, "{}", "MEDIUM".yellow()),
            ErrorSeverity::Low => write!(f, "{}", "LOW".blue()),
            ErrorSeverity::Warning => write!(f, "{}", "WARNING".yellow().italic()),
            ErrorSeverity::Info => write!(f, "{}", "INFO".cyan().italic()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub operation: String,
    pub suggestion: Option<String>,
    pub related_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReaperError {
    pub message: String,
    pub severity: ErrorSeverity,
    pub context: Option<ErrorContext>,
    pub source: Option<Box<ReaperError>>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl fmt::Display for ReaperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.severity, self.message)?;

        if let Some(ref ctx) = self.context {
            write!(f, "\n  Operation: {}", ctx.operation)?;
            if let Some(ref file) = ctx.file {
                write!(f, "\n  File: {}", file)?;
                if let Some(line) = ctx.line {
                    write!(f, ":{}", line)?;
                }
            }
            if let Some(ref suggestion) = ctx.suggestion {
                write!(f, "\n  💡 Suggestion: {}", suggestion.green())?;
            }
            if !ctx.related_errors.is_empty() {
                write!(f, "\n  Related errors:")?;
                for err in &ctx.related_errors {
                    write!(f, "\n    - {}", err)?;
                }
            }
        }

        if let Some(ref source) = self.source {
            write!(f, "\n  Caused by: {}", source)?;
        }

        Ok(())
    }
}

impl StdError for ReaperError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        None
    }
}

pub struct ErrorAggregator {
    errors: Arc<Mutex<VecDeque<ReaperError>>>,
    max_errors: usize,
    fail_fast: bool,
}

impl ErrorAggregator {
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: Arc::new(Mutex::new(VecDeque::new())),
            max_errors,
            fail_fast: false,
        }
    }

    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    #[allow(clippy::result_large_err)]
    pub fn add_error(&self, error: ReaperError) -> Result<(), ReaperError> {
        let mut errors = self.errors.lock().unwrap();

        if self.fail_fast
            && matches!(
                error.severity,
                ErrorSeverity::Critical | ErrorSeverity::High
            )
        {
            return Err(error);
        }

        if errors.len() >= self.max_errors {
            errors.pop_front();
        }

        errors.push_back(error);
        Ok(())
    }

    pub fn has_critical_errors(&self) -> bool {
        let errors = self.errors.lock().unwrap();
        errors
            .iter()
            .any(|e| matches!(e.severity, ErrorSeverity::Critical))
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.lock().unwrap().is_empty()
    }

    pub fn get_errors(&self) -> Vec<ReaperError> {
        self.errors.lock().unwrap().iter().cloned().collect()
    }

    pub fn get_errors_by_severity(&self, severity: ErrorSeverity) -> Vec<ReaperError> {
        let errors = self.errors.lock().unwrap();
        errors
            .iter()
            .filter(|e| std::mem::discriminant(&e.severity) == std::mem::discriminant(&severity))
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.errors.lock().unwrap().clear();
    }

    pub fn summary(&self) -> String {
        let errors = self.errors.lock().unwrap();
        let critical = errors
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::Critical))
            .count();
        let high = errors
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::High))
            .count();
        let medium = errors
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::Medium))
            .count();
        let low = errors
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::Low))
            .count();
        let warnings = errors
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::Warning))
            .count();
        let info = errors
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::Info))
            .count();

        format!(
            "Error Summary: {} critical, {} high, {} medium, {} low, {} warnings, {} info",
            critical.to_string().red().bold(),
            high.to_string().red(),
            medium.to_string().yellow(),
            low.to_string().blue(),
            warnings.to_string().yellow().italic(),
            info.to_string().cyan().italic()
        )
    }

    pub fn display_all(&self) {
        let errors = self.errors.lock().unwrap();
        if errors.is_empty() {
            println!("{}", "No errors recorded.".green());
            return;
        }

        println!("{}", "=== Error Report ===".bold());
        println!("{}", self.summary());
        println!();

        for (idx, error) in errors.iter().enumerate() {
            println!("{} Error #{}", "►".red().bold(), idx + 1);
            println!("{}", error);
            println!();
        }
    }
}

type RecoveryStrategy = Box<dyn Fn(&ReaperError) -> Option<String> + Send + Sync>;

pub struct ErrorRecovery {
    strategies: Vec<RecoveryStrategy>,
}

impl Default for ErrorRecovery {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorRecovery {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
        }
    }

    pub fn add_strategy<F>(mut self, strategy: F) -> Self
    where
        F: Fn(&ReaperError) -> Option<String> + Send + Sync + 'static,
    {
        self.strategies.push(Box::new(strategy));
        self
    }

    pub fn recover(&self, error: &ReaperError) -> Option<String> {
        for strategy in &self.strategies {
            if let Some(recovery) = strategy(error) {
                return Some(recovery);
            }
        }
        None
    }

    pub fn default_strategies() -> Self {
        Self::new()
            .add_strategy(|error| {
                if error.message.contains("Permission denied") {
                    Some("Try running with elevated privileges (sudo)".to_string())
                } else {
                    None
                }
            })
            .add_strategy(|error| {
                if error.message.contains("Network") || error.message.contains("connection") {
                    Some("Check your internet connection and try again".to_string())
                } else {
                    None
                }
            })
            .add_strategy(|error| {
                if error.message.contains("not found") {
                    Some(
                        "Ensure the package name is correct and try updating the database"
                            .to_string(),
                    )
                } else {
                    None
                }
            })
            .add_strategy(|error| {
                if error.message.contains("space") || error.message.contains("disk full") {
                    Some("Free up disk space and try again".to_string())
                } else {
                    None
                }
            })
    }
}

#[macro_export]
macro_rules! reaper_error {
    ($severity:expr, $msg:expr) => {
        ReaperError {
            message: $msg.to_string(),
            severity: $severity,
            context: None,
            source: None,
            timestamp: chrono::Utc::now(),
        }
    };
    ($severity:expr, $msg:expr, $ctx:expr) => {
        ReaperError {
            message: $msg.to_string(),
            severity: $severity,
            context: Some($ctx),
            source: None,
            timestamp: chrono::Utc::now(),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_aggregation() {
        let aggregator = ErrorAggregator::new(5);

        for i in 0..7 {
            let error = reaper_error!(ErrorSeverity::Medium, format!("Test error {}", i));
            aggregator.add_error(error).unwrap();
        }

        let errors = aggregator.get_errors();
        assert_eq!(errors.len(), 5);
        assert_eq!(errors[0].message, "Test error 2");
    }

    #[test]
    fn test_fail_fast() {
        let aggregator = ErrorAggregator::new(10).with_fail_fast(true);

        let low_error = reaper_error!(ErrorSeverity::Low, "Low severity");
        assert!(aggregator.add_error(low_error).is_ok());

        let critical_error = reaper_error!(ErrorSeverity::Critical, "Critical error");
        assert!(aggregator.add_error(critical_error).is_err());
    }

    #[test]
    fn test_error_recovery() {
        let recovery = ErrorRecovery::default_strategies();

        let perm_error = reaper_error!(ErrorSeverity::High, "Permission denied");
        let suggestion = recovery.recover(&perm_error);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("sudo"));
    }
}
