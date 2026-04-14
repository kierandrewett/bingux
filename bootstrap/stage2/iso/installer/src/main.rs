use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs},
};

/// The screens in the installer wizard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Welcome,
    DiskSelection,
    PackageSets,
    UserCreation,
    Installing,
    Complete,
}

impl Screen {
    const ALL: &[Screen] = &[
        Screen::Welcome,
        Screen::DiskSelection,
        Screen::PackageSets,
        Screen::UserCreation,
        Screen::Installing,
        Screen::Complete,
    ];

    fn index(self) -> usize {
        Screen::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    fn title(self) -> &'static str {
        match self {
            Screen::Welcome => "Welcome",
            Screen::DiskSelection => "Disk Selection",
            Screen::PackageSets => "Package Sets",
            Screen::UserCreation => "User Creation",
            Screen::Installing => "Installing",
            Screen::Complete => "Complete",
        }
    }
}

/// Installer application state.
struct App {
    screen: Screen,
    running: bool,

    // Disk selection
    disks: Vec<String>,
    selected_disk: usize,

    // Package sets
    package_sets: Vec<(String, bool)>,

    // User creation
    username: String,
    password: String,
    editing_field: usize, // 0 = username, 1 = password

    // Installation progress
    progress: f64,
    progress_message: String,
}

impl App {
    fn new() -> Self {
        Self {
            screen: Screen::Welcome,
            running: true,

            disks: vec![
                "/dev/sda  64 GB  ATA VBOX HARDDISK".into(),
                "/dev/sdb  128 GB  Samsung SSD 970".into(),
                "/dev/nvme0n1  512 GB  WD Black SN750".into(),
            ],
            selected_disk: 0,

            package_sets: vec![
                ("essential  (core system)".into(), true),
                ("base       (utilities + networking)".into(), true),
                ("desktop    (GNOME + Wayland)".into(), false),
                ("extras     (firefox, neovim, etc.)".into(), false),
            ],

            username: String::new(),
            password: String::new(),
            editing_field: 0,

            progress: 0.0,
            progress_message: "Waiting to start...".into(),
        }
    }

    fn next_screen(&mut self) {
        let idx = self.screen.index();
        if idx + 1 < Screen::ALL.len() {
            self.screen = Screen::ALL[idx + 1];
            if self.screen == Screen::Installing {
                self.progress = 0.0;
                self.progress_message = "Installing packages...".into();
            }
        }
    }

    fn prev_screen(&mut self) {
        let idx = self.screen.index();
        if idx > 0 && self.screen != Screen::Installing && self.screen != Screen::Complete {
            self.screen = Screen::ALL[idx - 1];
        }
    }

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') if self.screen != Screen::UserCreation => {
                self.running = false;
            }
            _ => match self.screen {
                Screen::Welcome => {
                    if matches!(key, KeyCode::Enter | KeyCode::Right) {
                        self.next_screen();
                    }
                }
                Screen::DiskSelection => match key {
                    KeyCode::Up => {
                        self.selected_disk = self.selected_disk.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if self.selected_disk + 1 < self.disks.len() {
                            self.selected_disk += 1;
                        }
                    }
                    KeyCode::Enter | KeyCode::Right => self.next_screen(),
                    KeyCode::Left => self.prev_screen(),
                    _ => {}
                },
                Screen::PackageSets => match key {
                    KeyCode::Up => {
                        // Find previous non-essential set
                        if let Some(pos) = (0..self.package_sets.len())
                            .rev()
                            .find(|&i| i < self.selected_disk)
                        {
                            self.selected_disk = pos;
                        }
                    }
                    KeyCode::Down => {
                        if self.selected_disk + 1 < self.package_sets.len() {
                            self.selected_disk += 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        // Toggle selection (essential is always on)
                        if self.selected_disk > 0 {
                            let enabled = &mut self.package_sets[self.selected_disk].1;
                            *enabled = !*enabled;
                        }
                    }
                    KeyCode::Enter | KeyCode::Right => {
                        self.selected_disk = 0;
                        self.next_screen();
                    }
                    KeyCode::Left => {
                        self.selected_disk = 0;
                        self.prev_screen();
                    }
                    _ => {}
                },
                Screen::UserCreation => match key {
                    KeyCode::Tab => {
                        self.editing_field = (self.editing_field + 1) % 2;
                    }
                    KeyCode::Backspace => {
                        let field = if self.editing_field == 0 {
                            &mut self.username
                        } else {
                            &mut self.password
                        };
                        field.pop();
                    }
                    KeyCode::Char(c) => {
                        let field = if self.editing_field == 0 {
                            &mut self.username
                        } else {
                            &mut self.password
                        };
                        field.push(c);
                    }
                    KeyCode::Enter => self.next_screen(),
                    KeyCode::Esc => self.prev_screen(),
                    _ => {}
                },
                Screen::Installing => {
                    // Simulate progress on any key
                    if self.progress < 1.0 {
                        self.progress = (self.progress + 0.1).min(1.0);
                        self.progress_message = match self.progress {
                            p if p < 0.3 => "Partitioning disk...".into(),
                            p if p < 0.6 => "Installing packages...".into(),
                            p if p < 0.8 => "Configuring system...".into(),
                            p if p < 1.0 => "Installing bootloader...".into(),
                            _ => "Installation complete!".into(),
                        };
                    }
                    if self.progress >= 1.0 {
                        self.next_screen();
                    }
                }
                Screen::Complete => {
                    if matches!(key, KeyCode::Enter) {
                        self.running = false;
                    }
                }
            },
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(0),   // content
            Constraint::Length(3), // help bar
        ])
        .split(frame.area());

    // Tab bar
    let titles: Vec<&str> = Screen::ALL.iter().map(|s| s.title()).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Bingux Installer "))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .select(app.screen.index());
    frame.render_widget(tabs, chunks[0]);

    // Content area
    match app.screen {
        Screen::Welcome => draw_welcome(frame, chunks[1]),
        Screen::DiskSelection => draw_disk_selection(frame, chunks[1], app),
        Screen::PackageSets => draw_package_sets(frame, chunks[1], app),
        Screen::UserCreation => draw_user_creation(frame, chunks[1], app),
        Screen::Installing => draw_installing(frame, chunks[1], app),
        Screen::Complete => draw_complete(frame, chunks[1]),
    }

    // Help bar
    let help = match app.screen {
        Screen::Welcome => "Press Enter to continue | q to quit",
        Screen::DiskSelection => "↑↓ select | Enter next | ← back | q quit",
        Screen::PackageSets => "↑↓ select | Space toggle | Enter next | ← back | q quit",
        Screen::UserCreation => "Tab switch field | Enter next | Esc back",
        Screen::Installing => "Press any key to simulate progress...",
        Screen::Complete => "Press Enter to reboot",
    };
    let help_bar = Paragraph::new(help)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help_bar, chunks[2]);
}

fn draw_welcome(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Welcome to Bingux",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("This installer will guide you through setting up Bingux"),
        Line::from("on your computer."),
        Line::from(""),
        Line::from("You will:"),
        Line::from("  1. Select a disk to install to"),
        Line::from("  2. Choose package sets"),
        Line::from("  3. Create a user account"),
        Line::from("  4. Watch the installation"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to begin.",
            Style::default().fg(Color::Green),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_disk_selection(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .disks
        .iter()
        .enumerate()
        .map(|(i, disk)| {
            let marker = if i == app.selected_disk { "▸ " } else { "  " };
            let style = if i == app.selected_disk {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{marker}{disk}")).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Select Installation Disk "),
    );
    frame.render_widget(list, area);
}

fn draw_package_sets(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .package_sets
        .iter()
        .enumerate()
        .map(|(i, (name, enabled))| {
            let checkbox = if *enabled { "[x]" } else { "[ ]" };
            let marker = if i == app.selected_disk { "▸" } else { " " };
            let style = if i == app.selected_disk {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if *enabled {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(format!("{marker} {checkbox} {name}")).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Package Sets "),
    );
    frame.render_widget(list, area);
}

fn draw_user_creation(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" Create User Account ");
    frame.render_widget(outer, area);

    let username_style = if app.editing_field == 0 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let username = Paragraph::new(app.username.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Username ")
            .border_style(username_style),
    );
    frame.render_widget(username, chunks[0]);

    let password_style = if app.editing_field == 1 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let masked: String = "*".repeat(app.password.len());
    let password = Paragraph::new(masked.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Password ")
            .border_style(password_style),
    );
    frame.render_widget(password, chunks[2]);

    let hint = Paragraph::new("Tab to switch fields, Enter to continue")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[4]);
}

fn draw_installing(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" Installation Progress ");
    frame.render_widget(outer, area);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.progress);
    frame.render_widget(gauge, chunks[1]);

    let msg = Paragraph::new(app.progress_message.as_str())
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White));
    frame.render_widget(msg, chunks[3]);
}

fn draw_complete(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "Installation Complete!",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Bingux has been installed successfully."),
        Line::from(""),
        Line::from("Remove the installation media and press Enter to reboot."),
    ];

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(" Done "));
    frame.render_widget(paragraph, area);
}

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut app = App::new();

    // Main loop
    while app.running {
        terminal.draw(|frame| draw(frame, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                app.handle_key(key.code);
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
