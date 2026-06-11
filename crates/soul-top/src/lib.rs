//! soul-top — Real-time TUI visualizer for SoulSystem.
//!
//! Displays organ health, turbulence, neural mesh connections,
//! and bus activity in a terminal dashboard.
//!
//! # Usage
//!
//! ```bash
//! soul-top [--endpoint http://127.0.0.1:9023] [--refresh 500]
//! ```

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

// ── Data Model ──────────────────────────────────────────────────────────

/// Health status of a single organ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganStatus {
    pub name: String,
    pub healthy: bool,
    pub uptime_s: u64,
    pub request_count: u64,
    pub avg_latency_ms: f64,
    pub vram_used_mib: u64,
    pub vram_total_mib: u64,
}

/// Brain event from the bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainEvent {
    pub timestamp: String,
    pub kind: String,
    pub source: String,
    pub payload: String,
}

/// Turbulence entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurbulenceEntry {
    pub timestamp: String,
    pub severity: String,
    pub message: String,
}

/// Aggregate health state for the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardState {
    pub organs: Vec<OrganStatus>,
    pub events: VecDeque<BrainEvent>,
    pub turbulence: VecDeque<TurbulenceEntry>,
    pub total_healthy: usize,
    pub total_organs: usize,
    pub bus_events_per_min: f64,
    pub uptime_s: u64,
}

impl DashboardState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new brain event (keeps last 50).
    pub fn push_event(&mut self, event: BrainEvent) {
        self.events.push_back(event);
        while self.events.len() > 50 {
            self.events.pop_front();
        }
    }

    /// Push a turbulence entry (keeps last 20).
    pub fn push_turbulence(&mut self, entry: TurbulenceEntry) {
        self.turbulence.push_back(entry);
        while self.turbulence.len() > 20 {
            self.turbulence.pop_front();
        }
    }

    /// Recompute aggregate stats.
    pub fn recompute(&mut self) {
        self.total_organs = self.organs.len();
        self.total_healthy = self.organs.iter().filter(|o| o.healthy).count();
    }
}

// ── TUI Renderer ────────────────────────────────────────────────────────

/// Render the full dashboard.
pub fn render_dashboard(f: &mut Frame, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(5), // gauges
            Constraint::Min(8),    // organs + events
            Constraint::Length(6), // turbulence
        ])
        .split(f.area());

    render_header(f, chunks[0], state);
    render_gauges(f, chunks[1], state);
    render_organs_and_events(f, chunks[2], state);
    render_turbulence(f, chunks[3], state);
}

fn render_header(f: &mut Frame, area: Rect, state: &DashboardState) {
    let title = format!(
        " SOUL-TOP │ {} │ {} │ Uptime: {}s ",
        chrono::Local::now().format("%H:%M:%S"),
        if state.total_healthy == state.total_organs {
            "ALL HEALTHY"
        } else {
            "DEGRADED"
        },
        state.uptime_s,
    );
    let style = if state.total_healthy == state.total_organs {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let header = Paragraph::new(Line::from(Span::styled(title, style))).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(header, area);
}

fn render_gauges(f: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);

    // Health gauge
    let health_ratio = if state.total_organs > 0 {
        state.total_healthy as f64 / state.total_organs as f64
    } else {
        0.0
    };
    let health_color = if health_ratio >= 1.0 {
        Color::Green
    } else if health_ratio >= 0.7 {
        Color::Yellow
    } else {
        Color::Red
    };
    let health_gauge = Gauge::default()
        .block(Block::default().title("Health").borders(Borders::ALL))
        .gauge_style(Style::default().fg(health_color))
        .ratio(health_ratio)
        .label(format!("{}/{}", state.total_healthy, state.total_organs));
    f.render_widget(health_gauge, chunks[0]);

    // Bus events gauge
    let bus_gauge = Gauge::default()
        .block(
            Block::default()
                .title("Bus Events/min")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .ratio((state.bus_events_per_min / 100.0).min(1.0))
        .label(format!("{:.0}", state.bus_events_per_min));
    f.render_widget(bus_gauge, chunks[1]);

    // Organ count
    let organ_info = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("Organs: {}", state.total_organs),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Healthy: {}", state.total_healthy),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Events: {}", state.events.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().title("Summary").borders(Borders::ALL));
    f.render_widget(organ_info, chunks[2]);
}

fn render_organs_and_events(f: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Organ list
    let organ_items: Vec<ListItem> = state
        .organs
        .iter()
        .map(|o| {
            let color = if o.healthy { Color::Green } else { Color::Red };
            let status = if o.healthy { "♥" } else { "✗" };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", status), Style::default().fg(color)),
                Span::styled(&o.name, Style::default().fg(Color::White)),
                Span::styled(
                    format!(
                        "  {}ms  {}MiB VRAM",
                        o.avg_latency_ms as u64, o.vram_used_mib
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let organ_list = List::new(organ_items).block(
        Block::default()
            .title("Organs")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(organ_list, chunks[0]);

    // Recent events
    let event_items: Vec<ListItem> = state
        .events
        .iter()
        .rev()
        .take(15)
        .map(|e| {
            let kind_color = match e.kind.as_str() {
                "Heartbeat" => Color::Green,
                "Error" => Color::Red,
                "Turbulence" => Color::Yellow,
                _ => Color::DarkGray,
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{}] ", &e.timestamp[..8.min(e.timestamp.len())]),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(&e.kind, Style::default().fg(kind_color)),
                Span::styled(format!(" ← {}", e.source), Style::default().fg(Color::Blue)),
            ]))
        })
        .collect();

    let event_list = List::new(event_items).block(
        Block::default()
            .title("Recent Events")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(event_list, chunks[1]);
}

fn render_turbulence(f: &mut Frame, area: Rect, state: &DashboardState) {
    let items: Vec<ListItem> = state
        .turbulence
        .iter()
        .rev()
        .take(5)
        .map(|t| {
            let color = match t.severity.as_str() {
                "critical" => Color::Red,
                "high" => Color::Yellow,
                "medium" => Color::DarkGray,
                _ => Color::White,
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{}] ", &t.timestamp[..8.min(t.timestamp.len())]),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{:8}", t.severity), Style::default().fg(color)),
                Span::raw(format!(" {}", t.message)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("Turbulence")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(list, area);
}

// ── Demo Data ───────────────────────────────────────────────────────────

/// Generate demo data for testing the TUI.
pub fn demo_state() -> DashboardState {
    let mut state = DashboardState::new();

    state.organs = vec![
        OrganStatus {
            name: "soul-brain".into(),
            healthy: true,
            uptime_s: 86400,
            request_count: 12000,
            avg_latency_ms: 45.0,
            vram_used_mib: 2048,
            vram_total_mib: 8192,
        },
        OrganStatus {
            name: "soul-memory".into(),
            healthy: true,
            uptime_s: 86400,
            request_count: 8500,
            avg_latency_ms: 12.0,
            vram_used_mib: 1024,
            vram_total_mib: 8192,
        },
        OrganStatus {
            name: "soul-guardian".into(),
            healthy: true,
            uptime_s: 86400,
            request_count: 45000,
            avg_latency_ms: 2.0,
            vram_used_mib: 0,
            vram_total_mib: 0,
        },
        OrganStatus {
            name: "soul-router".into(),
            healthy: true,
            uptime_s: 86400,
            request_count: 67000,
            avg_latency_ms: 1.5,
            vram_used_mib: 0,
            vram_total_mib: 0,
        },
    ];

    state.push_event(BrainEvent {
        timestamp: "14:32:05".into(),
        kind: "Heartbeat".into(),
        source: "soul-brain".into(),
        payload: "ok".into(),
    });
    state.push_event(BrainEvent {
        timestamp: "14:32:04".into(),
        kind: "Heartbeat".into(),
        source: "soul-memory".into(),
        payload: "ok".into(),
    });
    state.push_event(BrainEvent {
        timestamp: "14:32:03".into(),
        kind: "Turbulence".into(),
        source: "soul-guardian".into(),
        payload: "latency spike 12ms".into(),
    });

    state.push_turbulence(TurbulenceEntry {
        timestamp: "14:30:00".into(),
        severity: "low".into(),
        message: "soul-memory latency 15ms → 22ms".into(),
    });

    state.recompute();
    state.bus_events_per_min = 120.0;
    state.uptime_s = 86400;

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_state_push_event() {
        let mut state = DashboardState::new();
        for i in 0..60 {
            state.push_event(BrainEvent {
                timestamp: format!("14:00:{:02}", i),
                kind: "Heartbeat".into(),
                source: "test".into(),
                payload: "".into(),
            });
        }
        assert_eq!(state.events.len(), 50);
    }

    #[test]
    fn test_dashboard_state_push_turbulence() {
        let mut state = DashboardState::new();
        for i in 0..25 {
            state.push_turbulence(TurbulenceEntry {
                timestamp: format!("14:00:{:02}", i),
                severity: "low".into(),
                message: format!("event {}", i),
            });
        }
        assert_eq!(state.turbulence.len(), 20);
    }

    #[test]
    fn test_demo_state() {
        let state = demo_state();
        assert!(state.total_organs > 0);
        assert_eq!(state.total_healthy, state.total_organs);
    }
}
