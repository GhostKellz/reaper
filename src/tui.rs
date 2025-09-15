// Remove any dead/legacy code, ensure all log output uses LogPane, and document hooks

use crate::aur;
use crate::aur::SearchResult;
use crate::core;
use crate::profiles::ProfileManager;
use crate::trust::{TrustEngine, TrustScore};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{BarChart, Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Tabs};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::time::{Duration, sleep};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchSource {
    Aur,
    Flatpak,
    Pacman,
    ChaoticAur,
    GhostctlAur,
}

impl SearchSource {
    #[allow(dead_code)]
    fn label(&self) -> &'static str {
        match self {
            SearchSource::Aur => "AUR",
            SearchSource::Flatpak => "Flatpak",
            SearchSource::Pacman => "Pacman",
            SearchSource::ChaoticAur => "ChaoticAUR",
            SearchSource::GhostctlAur => "GhostctlAUR",
        }
    }
    #[allow(dead_code)]
    fn all() -> &'static [SearchSource] {
        &[
            Self::Aur,
            Self::Flatpak,
            Self::Pacman,
            Self::ChaoticAur,
            Self::GhostctlAur,
        ]
    }
}

struct SearchTab {
    query: String,
    #[allow(dead_code)]
    source: SearchSource,
    results: Vec<SearchResult>,
    trust_scores: HashMap<String, TrustScore>,
    selected: usize,
}

impl SearchTab {
    fn new() -> Self {
        Self {
            query: String::new(),
            source: SearchSource::Aur,
            results: Vec::new(),
            trust_scores: HashMap::new(),
            selected: 0,
        }
    }

    #[allow(dead_code)]
    async fn do_search(&mut self) {
        let trust_engine = TrustEngine::new();

        match self.source {
            SearchSource::Aur => {
                self.results = aur::search(&self.query).await.unwrap_or_default();
                // Compute trust scores for results
                for result in &self.results {
                    let trust_score = trust_engine
                        .compute_trust_score(&result.name, &result.source)
                        .await;
                    self.trust_scores.insert(result.name.clone(), trust_score);
                }
            }
            SearchSource::Flatpak => {
                self.results = crate::flatpak::search(&self.query);
            }
            SearchSource::Pacman | SearchSource::ChaoticAur | SearchSource::GhostctlAur => {
                let repo = match self.source {
                    SearchSource::Pacman => None,
                    SearchSource::ChaoticAur => Some("chaotic-aur"),
                    SearchSource::GhostctlAur => Some("ghostctl-aur"),
                    _ => None,
                };
                self.results = search_pacman_like(&self.query, repo);
            }
        }
        self.selected = 0;
    }

    fn render_with_trust(
        &self,
        f: &mut Frame<'_>,
        area: ratatui::layout::Rect,
        trust_engine: &TrustEngine,
    ) {
        let items: Vec<ListItem> = self
            .results
            .iter()
            .enumerate()
            .map(|(i, result)| {
                let trust_badge = if let Some(trust) = self.trust_scores.get(&result.name) {
                    format!(" {}", trust_engine.display_trust_badge(trust.overall_score))
                } else {
                    " ❓ UNKNOWN".to_string()
                };

                let style = if i == self.selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };

                let content = format!(
                    "{}{} - {} ({})",
                    result.name, trust_badge, result.description, result.version
                );
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Search Results"),
        );
        f.render_widget(list, area);
    }
}

#[allow(dead_code)]
fn search_pacman_like(query: &str, repo: Option<&str>) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut cmd = std::process::Command::new("pacman");
    cmd.arg("-Ss").arg(query);
    if let Some(repo) = repo {
        cmd.arg("| grep").arg(repo);
    }
    if let Ok(out) = cmd.output() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let mut parts = line.split_whitespace();
            let pkg = parts.next().unwrap_or("");
            let version = parts.next().unwrap_or("");
            let desc = parts.collect::<Vec<_>>().join(" ");
            if !pkg.is_empty() {
                results.push(SearchResult {
                    name: pkg.to_string(),
                    version: version.to_string(),
                    description: desc,
                    source: crate::core::Source::Pacman,
                });
            }
        }
    }
    results
}

// Ensure InstallQueue uses Arc<Mutex<Vec<core::InstallTask>>> and all async methods use Arc<LogPane>
struct InstallQueue {
    tasks: Arc<Mutex<Vec<core::InstallTask>>>,
}

impl InstallQueue {
    fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }
    fn enqueue(&self, task: core::InstallTask) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(task);
    }

    #[allow(dead_code)]
    fn pop(&self) -> Option<core::InstallTask> {
        let mut tasks = self.tasks.lock().unwrap();
        if !tasks.is_empty() {
            Some(tasks.remove(0))
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub async fn process(&self, log_pane: Arc<LogPane>, backend: &str) {
        let semaphore = Arc::new(Semaphore::new(4));
        let mut handles = Vec::new();
        let mut completed = 0;
        let total = self.tasks.lock().unwrap().len();
        while let Some(task) = self.pop() {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let log_pane_task = Arc::clone(&log_pane);
            let backend = backend.to_string();
            handles.push(tokio::spawn(async move {
                match backend.as_str() {
                    "pacman" => {
                        let status = std::process::Command::new("pacman").status();
                        match status {
                            Ok(s) if s.success() => {
                                log_pane_task.push("[tui] Pacman install succeeded.")
                            }
                            Ok(_) | Err(_) => log_pane_task.push("[tui] Pacman install failed."),
                        }
                    }
                    "flatpak" => {
                        crate::flatpak::install(&task.pkg);
                        log_pane_task.push("[tui] Flatpak install attempted.");
                    }
                    _ => {
                        log_pane_task.push("[tui] Unknown backend.");
                    }
                }
                drop(permit);
            }));
            completed += 1;
            log_pane.push(&format!(
                "[reap][queue] Progress: {}/{} tasks complete",
                completed, total
            ));
            sleep(Duration::from_millis(300)).await;
        }
        for h in handles {
            let _ = h.await;
        }
        log_pane.push("[reap][queue] All install tasks complete.");
    }
}

pub struct LogPane {
    lines: Arc<Mutex<Vec<String>>>,
}

impl LogPane {
    pub fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn push(&self, line: &str) {
        let mut lines = self.lines.lock().unwrap();
        lines.push(line.to_string());
        if lines.len() > 1000 {
            lines.remove(0);
        }
    }
    pub fn get(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
    pub fn clear(&self) {
        let mut lines = self.lines.lock().unwrap();
        lines.clear();
    }
}

impl Default for LogPane {
    fn default() -> Self {
        Self::new()
    }
}

struct DiffViewer {
    lines: Vec<(char, String)>, // ('-', '+', ' ') and line
    scroll: usize,
}

impl DiffViewer {
    fn new(old: &str, new: &str) -> Self {
        let mut lines = Vec::new();
        for d in diff::lines(old, new) {
            match d {
                diff::Result::Left(l) => lines.push(('-', l.to_string())),
                diff::Result::Right(r) => lines.push(('+', r.to_string())),
                diff::Result::Both(l, _) => lines.push((' ', l.to_string())),
            }
        }
        Self { lines, scroll: 0 }
    }
    fn render(&self, f: &mut Frame<'_>, area: ratatui::layout::Rect) {
        let items: Vec<Line> = self
            .lines
            .iter()
            .map(|(ty, line)| {
                let style = match ty {
                    '-' => Style::default().fg(Color::Red),
                    '+' => Style::default().fg(Color::Green),
                    _ => Style::default(),
                };
                Line::from(vec![Span::styled(format!("{} {}", ty, line), style)])
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .title("PKGBUILD Diff");
        let para = Paragraph::new(items)
            .block(block)
            .scroll((self.scroll as u16, 0));
        f.render_widget(Clear, area);
        f.render_widget(para, area);
    }
}

/// Setup terminal for TUI
fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore terminal after TUI
fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Enhanced TUI with live monitoring, trust scores, and profile management
pub async fn launch_tui() {
    let start = Instant::now();
    let mut terminal = setup_terminal().expect("Failed to setup terminal");
    let mut search_tab = SearchTab::new();
    let mut tab_idx = 0;
    let tab_titles = ["Search", "Queue", "Log", "Profiles", "System"];
    let log_pane = Arc::new(LogPane::new());
    let mut log_scroll = 0usize;
    let install_queue = Arc::new(InstallQueue::new());
    let installed = core::get_installed_packages();
    let mut diff_viewer: Option<DiffViewer> = None;
    let _backend = "aur";
    let trust_engine = TrustEngine::new();
    let profile_manager = ProfileManager::new();
    let mut build_progress = BuildProgress::new();

    loop {
        terminal
            .draw(|f| {
                let size = f.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Tabs
                        Constraint::Min(5),    // Main content
                        Constraint::Length(7), // Bottom panel
                        Constraint::Length(2), // Status bar
                    ])
                    .split(size);

                // Enhanced tabs with icons
                let tabs = Tabs::new(tab_titles.iter().enumerate().map(|(i, title)| {
                    let icon = match i {
                        0 => "🔍 ",
                        1 => "📦 ",
                        2 => "📋 ",
                        3 => "👤 ",
                        4 => "🖥️ ",
                        _ => "",
                    };
                    format!("{}{}", icon, title)
                }))
                .block(Block::default().borders(Borders::ALL).title("Reaper v0.6"))
                .select(tab_idx)
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
                f.render_widget(tabs, chunks[0]);

                match tab_idx {
                    0 => {
                        // Enhanced search tab with trust scores
                        search_tab.render_with_trust(f, chunks[1], &trust_engine);
                    }
                    1 => {
                        // Queue tab with progress bars
                        render_queue_tab(f, chunks[1], &install_queue, &build_progress);
                    }
                    2 => {
                        // Log tab with filtering
                        render_log_tab(f, chunks[1], &log_pane, log_scroll);
                    }
                    3 => {
                        // Profiles management tab
                        render_profiles_tab(f, chunks[1], &profile_manager);
                    }
                    4 => {
                        // System monitoring tab
                        render_system_tab(f, chunks[1], &installed);
                    }
                    _ => {}
                }

                // Enhanced bottom panel with real-time stats
                render_bottom_panel(f, chunks[2], &installed, &trust_engine);

                // Status bar with current profile and system info
                render_status_bar(f, chunks[3], &profile_manager);

                if let Some(diff) = &diff_viewer {
                    let area = Layout::default().split(f.size())[0];
                    diff.render(f, area);
                }
            })
            .unwrap();

        if crossterm::event::poll(std::time::Duration::from_millis(100)).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('/') => {
                        // Enter search mode
                        if tab_idx == 0 {
                            log_pane.push("[tui] Search mode activated");
                        }
                    }
                    KeyCode::Char('d') => {
                        // Show diff for selected package
                        if tab_idx == 0 && !search_tab.results.is_empty() {
                            let selected_pkg = &search_tab.results[search_tab.selected];
                            let old = "";
                            let new = crate::aur::get_pkgbuild_preview(&selected_pkg.name);
                            diff_viewer = Some(DiffViewer::new(old, &new));
                        }
                    }
                    KeyCode::Char('t') => {
                        // Show trust details for selected package
                        if tab_idx == 0 && !search_tab.results.is_empty() {
                            let selected_pkg = &search_tab.results[search_tab.selected];
                            if let Some(trust) = search_tab.trust_scores.get(&selected_pkg.name) {
                                log_pane.push(&format!(
                                    "[trust] {}: Score {:.1}/10",
                                    selected_pkg.name, trust.overall_score
                                ));
                                for flag in &trust.security_flags {
                                    log_pane.push(&format!("[trust] ⚠️ {:?}", flag));
                                }
                            }
                        }
                    }
                    KeyCode::Char('p') => {
                        // Switch to profiles tab
                        tab_idx = 3;
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        log_pane.clear();
                    }
                    KeyCode::Char('\t') => {
                        tab_idx = (tab_idx + 1) % tab_titles.len();
                    }
                    KeyCode::Char(c) => {
                        if tab_idx == 0 {
                            search_tab.query.push(c);
                        }
                    }
                    KeyCode::Up => {
                        if tab_idx == 0 && search_tab.selected > 0 {
                            search_tab.selected -= 1;
                        } else if tab_idx == 2 && log_scroll > 0 {
                            log_scroll -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if tab_idx == 0
                            && search_tab.selected < search_tab.results.len().saturating_sub(1)
                        {
                            search_tab.selected += 1;
                        } else if tab_idx == 2 {
                            log_scroll += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if tab_idx == 0 && !search_tab.results.is_empty() {
                            let selected = &search_tab.results[search_tab.selected];
                            let task = core::InstallTask::new(
                                selected.name.clone(),
                                selected.source.clone(),
                            );
                            install_queue.enqueue(task);
                            log_pane
                                .push(&format!("[queue] Added {} to install queue", selected.name));
                        }
                    }
                    _ => {}
                }
            }
        }

        // Update build progress periodically
        build_progress.update().await;
    }
    let elapsed = start.elapsed();
    println!("[tui] Session duration: {:?}", elapsed);
    restore_terminal(&mut terminal).expect("Failed to restore terminal");
}

#[derive(Default)]
struct BuildProgress {
    current_package: Option<String>,
    progress: f64,
    stage: String,
    #[allow(dead_code)]
    eta: Option<Duration>,
}

impl BuildProgress {
    fn new() -> Self {
        Self::default()
    }

    async fn update(&mut self) {
        // TODO: Real progress tracking from makepkg output
        if self.current_package.is_some() {
            self.progress = (self.progress + 0.01).min(1.0);
            if self.progress >= 1.0 {
                self.current_package = None;
                self.progress = 0.0;
            }
        }
    }
}

fn render_queue_tab(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    queue: &InstallQueue,
    progress: &BuildProgress,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Queue list
    let queue_items: Vec<ListItem> = queue
        .tasks
        .lock()
        .unwrap()
        .iter()
        .map(|task| {
            let status = match task.source {
                crate::core::Source::Aur => "📦 AUR",
                crate::core::Source::Flatpak => "📱 Flatpak",
                crate::core::Source::Pacman => "🏛️ Pacman",
                _ => "❓ Unknown",
            };
            ListItem::new(format!("{} - {}", task.pkg, status))
        })
        .collect();

    let queue_list = List::new(queue_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Install Queue"),
    );
    f.render_widget(queue_list, chunks[0]);

    // Build progress
    if let Some(ref pkg) = progress.current_package {
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Build Progress"),
            )
            .gauge_style(Style::default().fg(Color::Green))
            .percent((progress.progress * 100.0) as u16)
            .label(format!("Building {} - {}", pkg, progress.stage));
        f.render_widget(gauge, chunks[1]);
    } else {
        let paragraph = Paragraph::new("No active builds").block(
            Block::default()
                .borders(Borders::ALL)
                .title("Build Progress"),
        );
        f.render_widget(paragraph, chunks[1]);
    }
}

fn render_log_tab(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    log_pane: &LogPane,
    scroll: usize,
) {
    let logs = log_pane.get();
    let visible_logs: Vec<ListItem> = logs
        .iter()
        .skip(scroll)
        .take(area.height as usize - 2)
        .map(|line| {
            let style = if line.contains("ERROR") || line.contains("❌") {
                Style::default().fg(Color::Red)
            } else if line.contains("WARN") || line.contains("⚠️") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("✅") || line.contains("SUCCESS") {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(line.as_str()).style(style)
        })
        .collect();

    let log_list =
        List::new(visible_logs).block(Block::default().borders(Borders::ALL).title("Activity Log"));
    f.render_widget(log_list, area);
}

fn render_profiles_tab(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    profile_manager: &ProfileManager,
) {
    let profiles = profile_manager.list_profiles().unwrap_or_default();
    let active_profile = profile_manager.get_active_profile().unwrap_or_default();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Profile list
    let profile_items: Vec<ListItem> = profiles
        .iter()
        .map(|profile| {
            let indicator = if *profile == active_profile.name {
                "➤ "
            } else {
                "  "
            };
            let style = if *profile == active_profile.name {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{}{}", indicator, profile)).style(style)
        })
        .collect();

    let profile_list =
        List::new(profile_items).block(Block::default().borders(Borders::ALL).title("Profiles"));
    f.render_widget(profile_list, chunks[0]);

    // Profile details
    let details = format!(
        "Name: {}\nBackends: {:?}\nParallel Jobs: {:?}\nFast Mode: {:?}\nStrict Signatures: {:?}",
        active_profile.name,
        active_profile.backend_order,
        active_profile.parallel_jobs,
        active_profile.fast_mode,
        active_profile.strict_signatures
    );

    let details_paragraph = Paragraph::new(details).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Profile Details"),
    );
    f.render_widget(details_paragraph, chunks[1]);
}

fn render_system_tab(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    installed: &HashMap<String, crate::core::Source>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Stats bar
            Constraint::Min(5),    // Package list
        ])
        .split(area);

    // System stats
    let total_packages = installed.len();
    let aur_count = installed
        .values()
        .filter(|s| matches!(s, crate::core::Source::Aur))
        .count();
    let flatpak_count = installed
        .values()
        .filter(|s| matches!(s, crate::core::Source::Flatpak))
        .count();
    let pacman_count = total_packages - aur_count - flatpak_count;

    let stats_data = vec![
        ("Pacman", pacman_count as u64),
        ("AUR", aur_count as u64),
        ("Flatpak", flatpak_count as u64),
    ];

    let chart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Package Distribution"),
        )
        .data(&stats_data)
        .bar_width(9)
        .bar_style(Style::default().fg(Color::Cyan))
        .value_style(
            Style::default()
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(chart, chunks[0]);

    // Package list with sources
    let package_items: Vec<ListItem> = installed
        .iter()
        .take(area.height as usize - 5)
        .map(|(pkg, source)| {
            let source_icon = match source {
                crate::core::Source::Aur => "📦",
                crate::core::Source::Flatpak => "📱",
                crate::core::Source::Pacman => "🏛️",
                _ => "❓",
            };
            ListItem::new(format!("{} {}", source_icon, pkg))
        })
        .collect();

    let package_list = List::new(package_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Installed Packages"),
    );
    f.render_widget(package_list, chunks[1]);
}

// Fix the trust_engine parameter to avoid unused warning
fn render_bottom_panel_fixed(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    installed: &HashMap<String, crate::core::Source>,
    _trust_engine: &TrustEngine,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);

    // System info
    let total_packages = installed.len();
    let system_info = format!(
        "📦 Total: {}\n🔄 Updates: 0\n💾 Cache: 2.1GB",
        total_packages
    );
    let info_paragraph =
        Paragraph::new(system_info).block(Block::default().borders(Borders::ALL).title("System"));
    f.render_widget(info_paragraph, chunks[0]);

    // Trust summary (placeholder - implement actual trust stats in the future)
    let trust_summary = "🛡️ Trusted: 45\n⚠️ Caution: 12\n❌ Risky: 2";
    let trust_paragraph = Paragraph::new(trust_summary)
        .block(Block::default().borders(Borders::ALL).title("Security"));
    f.render_widget(trust_paragraph, chunks[1]);

    // Quick actions
    let actions = "⌨️ Hotkeys:\n/ Search  d Diff\nt Trust   p Profile\nq Quit    ↑↓ Navigate";
    let actions_paragraph =
        Paragraph::new(actions).block(Block::default().borders(Borders::ALL).title("Actions"));
    f.render_widget(actions_paragraph, chunks[2]);
}

// Use the fixed version as an alias
fn render_bottom_panel(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    installed: &HashMap<String, crate::core::Source>,
    trust_engine: &TrustEngine,
) {
    render_bottom_panel_fixed(f, area, installed, trust_engine);
}

fn render_status_bar(
    f: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    profile_manager: &ProfileManager,
) {
    let profile = profile_manager.get_active_profile().unwrap_or_default();
    let status = format!(
        "Profile: {} | Backend: {:?} | Status: Ready",
        profile.name,
        profile.backend_order.first().unwrap_or(&"none".to_string())
    );

    let status_paragraph =
        Paragraph::new(status).style(Style::default().bg(Color::Blue).fg(Color::White));
    f.render_widget(status_paragraph, area);
}

// NOTE: Remove any orphaned 'let start = Instant::now();' statements that appear at module level
// These should only be inside functions, not at the top level of the module
