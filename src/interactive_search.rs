use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::io::{self, Write};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub description: String,
    pub votes: u32,
    pub popularity: f64,
    pub installed: bool,
    pub out_of_date: bool,
    pub maintainer: String,
    pub repo: String,
    pub size: String,
}

#[derive(Debug, Clone)]
pub enum SortMethod {
    Relevance,
    Name,
    Votes,
    Popularity,
    Size,
    Date,
}

impl std::fmt::Display for SortMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortMethod::Relevance => write!(f, "Relevance"),
            SortMethod::Name => write!(f, "Name"),
            SortMethod::Votes => write!(f, "Votes"),
            SortMethod::Popularity => write!(f, "Popularity"),
            SortMethod::Size => write!(f, "Size"),
            SortMethod::Date => write!(f, "Date"),
        }
    }
}

pub struct InteractiveSearch {
    results: Vec<SearchResult>,
    filtered_results: Vec<SearchResult>,
    selected_index: usize,
    marked_items: Vec<usize>,
    search_query: String,
    filter_query: String,
    sort_method: SortMethod,
    show_details: bool,
    page: usize,
    items_per_page: usize,
    fuzzy_matcher: SkimMatcherV2,
}

impl InteractiveSearch {
    pub fn new(results: Vec<SearchResult>) -> Self {
        let filtered_results = results.clone();
        Self {
            results,
            filtered_results,
            selected_index: 0,
            marked_items: Vec::new(),
            search_query: String::new(),
            filter_query: String::new(),
            sort_method: SortMethod::Relevance,
            show_details: false,
            page: 0,
            items_per_page: 10,
            fuzzy_matcher: SkimMatcherV2::default(),
        }
    }

    pub async fn run(&mut self) -> io::Result<Vec<SearchResult>> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;

        let result = self.event_loop().await;

        execute!(io::stdout(), LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;

        result
    }

    async fn event_loop(&mut self) -> io::Result<Vec<SearchResult>> {
        loop {
            self.render()?;

            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key_event) = event::read()?
            {
                match self.handle_input(key_event).await {
                    InputResult::Continue => continue,
                    InputResult::Quit => return Ok(Vec::new()),
                    InputResult::Select => {
                        let selected: Vec<SearchResult> = self
                            .marked_items
                            .iter()
                            .filter_map(|&idx| self.filtered_results.get(idx).cloned())
                            .collect();

                        if selected.is_empty() && self.selected_index < self.filtered_results.len()
                        {
                            return Ok(vec![self.filtered_results[self.selected_index].clone()]);
                        }
                        return Ok(selected);
                    }
                }
            }
        }
    }

    fn render(&self) -> io::Result<()> {
        execute!(io::stdout(), terminal::Clear(ClearType::All))?;
        execute!(io::stdout(), cursor::MoveTo(0, 0))?;

        self.render_header()?;
        self.render_search_bar()?;
        self.render_results()?;
        self.render_footer()?;

        if self.show_details {
            self.render_details()?;
        }

        io::stdout().flush()?;
        Ok(())
    }

    fn render_header(&self) -> io::Result<()> {
        execute!(io::stdout(), SetForegroundColor(Color::Cyan))?;
        execute!(
            io::stdout(),
            Print("╔═══════════════════════════════════════════════════════════════════╗\n")
        )?;
        execute!(
            io::stdout(),
            Print("║              REAPER - Interactive Package Search                   ║\n")
        )?;
        execute!(
            io::stdout(),
            Print("╚═══════════════════════════════════════════════════════════════════╝\n")
        )?;
        execute!(io::stdout(), ResetColor)?;
        Ok(())
    }

    fn render_search_bar(&self) -> io::Result<()> {
        execute!(io::stdout(), cursor::MoveTo(0, 4))?;
        execute!(io::stdout(), SetForegroundColor(Color::Green))?;
        execute!(
            io::stdout(),
            Print(format!("Search: {}", self.search_query))
        )?;

        if !self.filter_query.is_empty() {
            execute!(
                io::stdout(),
                Print(format!(" | Filter: {}", self.filter_query))
            )?;
        }

        execute!(
            io::stdout(),
            Print(format!(" | Sort: {} ", self.sort_method))
        )?;
        execute!(
            io::stdout(),
            Print(format!(
                "| Page: {}/{} ",
                self.page + 1,
                self.filtered_results.len().div_ceil(self.items_per_page)
            ))
        )?;
        execute!(io::stdout(), ResetColor)?;
        execute!(io::stdout(), Print("\n"))?;
        execute!(io::stdout(), Print("─".repeat(70)))?;
        execute!(io::stdout(), Print("\n"))?;
        Ok(())
    }

    fn render_results(&self) -> io::Result<()> {
        let start_idx = self.page * self.items_per_page;
        let end_idx = (start_idx + self.items_per_page).min(self.filtered_results.len());

        for (idx, result) in self.filtered_results[start_idx..end_idx].iter().enumerate() {
            let global_idx = start_idx + idx;
            let is_selected = global_idx == self.selected_index;
            let is_marked = self.marked_items.contains(&global_idx);

            execute!(io::stdout(), cursor::MoveTo(0, (7 + idx) as u16))?;

            if is_selected {
                execute!(io::stdout(), SetForegroundColor(Color::Black))?;
                execute!(
                    io::stdout(),
                    crossterm::style::SetBackgroundColor(Color::White)
                )?;
            }

            let mark_indicator = if is_marked { "[✓]" } else { "[ ]" };
            let installed_indicator = if result.installed { "●" } else { " " };
            let ood_indicator = if result.out_of_date { "⚠" } else { " " };

            let name_styled = result.name.clone().bold();
            let repo_styled = result.repo.clone().blue();
            let installed_styled = installed_indicator.green();
            let ood_styled = ood_indicator.yellow();

            execute!(
                io::stdout(),
                Print(format!(
                    "{} {} {} {} {} (v{}) - {}",
                    mark_indicator,
                    installed_styled,
                    ood_styled,
                    repo_styled,
                    name_styled,
                    result.version,
                    result.description.chars().take(40).collect::<String>()
                ))
            )?;

            if is_selected {
                execute!(io::stdout(), crossterm::style::ResetColor)?;
            }
        }

        Ok(())
    }

    fn render_details(&self) -> io::Result<()> {
        if self.selected_index >= self.filtered_results.len() {
            return Ok(());
        }

        let result = &self.filtered_results[self.selected_index];
        let detail_y = 20;

        execute!(io::stdout(), cursor::MoveTo(0, detail_y))?;
        execute!(io::stdout(), Print("─".repeat(70)))?;
        execute!(io::stdout(), Print("\n"))?;

        execute!(io::stdout(), SetForegroundColor(Color::Cyan))?;
        let package_name_styled = result.name.clone().bold();
        execute!(
            io::stdout(),
            Print(format!("Package: {}\n", package_name_styled))
        )?;
        execute!(io::stdout(), ResetColor)?;

        execute!(
            io::stdout(),
            Print(format!("Version: {}\n", result.version))
        )?;
        execute!(
            io::stdout(),
            Print(format!("Repository: {}\n", result.repo))
        )?;
        execute!(
            io::stdout(),
            Print(format!("Maintainer: {}\n", result.maintainer))
        )?;
        execute!(
            io::stdout(),
            Print(format!(
                "Votes: {} | Popularity: {:.2}\n",
                result.votes, result.popularity
            ))
        )?;
        execute!(io::stdout(), Print(format!("Size: {}\n", result.size)))?;
        execute!(
            io::stdout(),
            Print(format!("Description:\n{}\n", result.description))
        )?;

        if result.installed {
            execute!(io::stdout(), SetForegroundColor(Color::Green))?;
            execute!(io::stdout(), Print("Status: INSTALLED\n"))?;
        }

        if result.out_of_date {
            execute!(io::stdout(), SetForegroundColor(Color::Yellow))?;
            execute!(io::stdout(), Print("Warning: Package is out of date\n"))?;
        }

        execute!(io::stdout(), ResetColor)?;

        Ok(())
    }

    fn render_footer(&self) -> io::Result<()> {
        let footer_y = 30;
        execute!(io::stdout(), cursor::MoveTo(0, footer_y))?;
        execute!(io::stdout(), Print("─".repeat(70)))?;
        execute!(io::stdout(), Print("\n"))?;

        execute!(io::stdout(), SetForegroundColor(Color::DarkGrey))?;
        execute!(
            io::stdout(),
            Print("↑/↓: Navigate | Space: Mark | Enter: Install | d: Details | s: Sort\n")
        )?;
        execute!(
            io::stdout(),
            Print("f: Filter | /: Search | PgUp/PgDn: Page | q: Quit | a: Mark All\n")
        )?;
        execute!(io::stdout(), ResetColor)?;

        Ok(())
    }

    async fn handle_input(&mut self, key_event: KeyEvent) -> InputResult {
        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => InputResult::Quit,
            KeyCode::Enter => InputResult::Select,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.update_page();
                }
                InputResult::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.filtered_results.len().saturating_sub(1) {
                    self.selected_index += 1;
                    self.update_page();
                }
                InputResult::Continue
            }
            KeyCode::Char(' ') => {
                if self.selected_index < self.filtered_results.len() {
                    if self.marked_items.contains(&self.selected_index) {
                        self.marked_items.retain(|&x| x != self.selected_index);
                    } else {
                        self.marked_items.push(self.selected_index);
                    }
                }
                InputResult::Continue
            }
            KeyCode::Char('d') => {
                self.show_details = !self.show_details;
                InputResult::Continue
            }
            KeyCode::Char('s') => {
                self.cycle_sort_method();
                self.apply_sort();
                InputResult::Continue
            }
            KeyCode::Char('f') => {
                self.enter_filter_mode().await;
                InputResult::Continue
            }
            KeyCode::Char('/') => {
                self.enter_search_mode().await;
                InputResult::Continue
            }
            KeyCode::PageUp => {
                if self.page > 0 {
                    self.page -= 1;
                    self.selected_index = self.page * self.items_per_page;
                }
                InputResult::Continue
            }
            KeyCode::PageDown => {
                let max_page =
                    (self.filtered_results.len().saturating_sub(1)) / self.items_per_page;
                if self.page < max_page {
                    self.page += 1;
                    self.selected_index = self.page * self.items_per_page;
                }
                InputResult::Continue
            }
            KeyCode::Char('a') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                for i in 0..self.filtered_results.len() {
                    if !self.marked_items.contains(&i) {
                        self.marked_items.push(i);
                    }
                }
                InputResult::Continue
            }
            _ => InputResult::Continue,
        }
    }

    fn update_page(&mut self) {
        if self.selected_index < self.page * self.items_per_page
            || self.selected_index >= (self.page + 1) * self.items_per_page
        {
            self.page = self.selected_index / self.items_per_page;
        }
    }

    fn cycle_sort_method(&mut self) {
        self.sort_method = match self.sort_method {
            SortMethod::Relevance => SortMethod::Name,
            SortMethod::Name => SortMethod::Votes,
            SortMethod::Votes => SortMethod::Popularity,
            SortMethod::Popularity => SortMethod::Size,
            SortMethod::Size => SortMethod::Date,
            SortMethod::Date => SortMethod::Relevance,
        };
    }

    fn apply_sort(&mut self) {
        match self.sort_method {
            SortMethod::Name => self.filtered_results.sort_by(|a, b| a.name.cmp(&b.name)),
            SortMethod::Votes => self
                .filtered_results
                .sort_by_key(|r| std::cmp::Reverse(r.votes)),
            SortMethod::Popularity => self.filtered_results.sort_by(|a, b| {
                b.popularity
                    .partial_cmp(&a.popularity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortMethod::Size => self.filtered_results.sort_by(|a, b| a.size.cmp(&b.size)),
            _ => {}
        }
    }

    async fn enter_filter_mode(&mut self) {
        let _ = execute!(io::stdout(), cursor::MoveTo(0, 35));
        let _ = execute!(io::stdout(), Print("Enter filter (ESC to cancel): "));
        let _ = io::stdout().flush();

        let mut filter = String::new();
        // Stop the input loop if the terminal event stream is unreadable
        // instead of panicking.
        while let Ok(ev) = event::read() {
            if let Event::Key(key) = ev {
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Enter => {
                        self.filter_query = filter;
                        self.apply_filter();
                        break;
                    }
                    KeyCode::Backspace => {
                        filter.pop();
                        let _ = execute!(io::stdout(), cursor::MoveTo(30, 35));
                        let _ = execute!(io::stdout(), terminal::Clear(ClearType::UntilNewLine));
                        let _ = execute!(io::stdout(), Print(&filter));
                    }
                    KeyCode::Char(c) => {
                        filter.push(c);
                        let _ = execute!(io::stdout(), Print(c));
                    }
                    _ => {}
                }
                let _ = io::stdout().flush();
            }
        }
    }

    async fn enter_search_mode(&mut self) {
        let _ = execute!(io::stdout(), cursor::MoveTo(0, 35));
        let _ = execute!(io::stdout(), Print("Enter search (ESC to cancel): "));
        let _ = io::stdout().flush();

        let mut search = String::new();
        while let Ok(ev) = event::read() {
            if let Event::Key(key) = ev {
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Enter => {
                        self.search_query = search;
                        break;
                    }
                    KeyCode::Backspace => {
                        search.pop();
                        let _ = execute!(io::stdout(), cursor::MoveTo(30, 35));
                        let _ = execute!(io::stdout(), terminal::Clear(ClearType::UntilNewLine));
                        let _ = execute!(io::stdout(), Print(&search));
                    }
                    KeyCode::Char(c) => {
                        search.push(c);
                        let _ = execute!(io::stdout(), Print(c));
                    }
                    _ => {}
                }
                let _ = io::stdout().flush();
            }
        }
    }

    fn apply_filter(&mut self) {
        if self.filter_query.is_empty() {
            self.filtered_results = self.results.clone();
        } else {
            self.filtered_results = self
                .results
                .iter()
                .filter(|r| {
                    let text = format!("{} {} {}", r.name, r.description, r.repo).to_lowercase();
                    let query = self.filter_query.to_lowercase();

                    if let Some(_score) = self.fuzzy_matcher.fuzzy_match(&text, &query) {
                        return true;
                    }

                    text.contains(&query)
                })
                .cloned()
                .collect();
        }

        self.selected_index = 0;
        self.page = 0;
        self.marked_items.clear();
    }
}

enum InputResult {
    Continue,
    Quit,
    Select,
}

pub async fn interactive_search(packages: Vec<SearchResult>) -> io::Result<Vec<SearchResult>> {
    let mut search = InteractiveSearch::new(packages);
    search.run().await
}
