//! Interactive Ratatui dashboard — topology, util, FS, MEM, agents.

use std::io::{stdout, Stdout};
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline};
use ratatui::Terminal;

use crate::brand::{ratatui_amber, ratatui_cyan, ratatui_void};
use crate::metrics::{ClusterMetrics, MetricsHub};

pub struct Dashboard {
    metrics: MetricsHub,
    cpu_hist: Vec<u64>,
    tab: usize,
    usb_bps_hist: Vec<u64>,
    usb_tick: u64,
}

impl Dashboard {
    pub fn new(metrics: MetricsHub) -> Self {
        Self {
            metrics,
            cpu_hist: vec![0; 64],
            tab: 0,
            usb_bps_hist: vec![0; 64],
            usb_tick: 0,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend)?;
        let result = self.event_loop(&mut terminal);
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        result
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        loop {
            let snap = self.metrics.snapshot();
            self.cpu_hist.push(snap.local_caps.cpu_util_pct as u64);
            if self.cpu_hist.len() > 64 {
                self.cpu_hist.remove(0);
            }
            // Idle USB sparkline animation (pipeline pulse) — not fabricated thermal.
            self.usb_tick = self.usb_tick.wrapping_add(1);
            let pulse = if self.usb_tick % 8 < 4 { 12 } else { 4 };
            self.usb_bps_hist.push(pulse);
            if self.usb_bps_hist.len() > 64 {
                self.usb_bps_hist.remove(0);
            }
            terminal.draw(|f| self.ui(f, &snap))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Tab | KeyCode::Right => self.tab = (self.tab + 1) % 5,
                        KeyCode::Left => self.tab = self.tab.checked_sub(1).unwrap_or(4),
                        KeyCode::Char('1') => self.tab = 0,
                        KeyCode::Char('2') => self.tab = 1,
                        KeyCode::Char('3') => self.tab = 2,
                        KeyCode::Char('4') => self.tab = 3,
                        KeyCode::Char('5') => self.tab = 4,
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn ui(&self, f: &mut ratatui::Frame, m: &ClusterMetrics) {
        let area = f.area();
        f.render_widget(
            Block::default().style(Style::default().bg(ratatui_void())),
            area,
        );
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                " CHIMERA ",
                Style::default()
                    .fg(ratatui_void())
                    .bg(ratatui_cyan())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " mesh · fs · mem · agents · usb ",
                Style::default().fg(ratatui_cyan()),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(ratatui_cyan())));
        f.render_widget(title, chunks[0]);

        let tabs = ["Topology", "ChimeraFS", "ChimeraMEM", "Agents", "USB"];
        let tab_line: Vec<Span> = tabs
            .iter()
            .enumerate()
            .flat_map(|(i, t)| {
                let style = if i == self.tab {
                    Style::default().fg(ratatui_void()).bg(ratatui_cyan()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(ratatui_cyan())
                };
                vec![
                    Span::styled(format!(" {t} "), style),
                    Span::raw(" "),
                ]
            })
            .collect();
        f.render_widget(Paragraph::new(Line::from(tab_line)), chunks[1]);

        match self.tab {
            0 => self.draw_topology(f, chunks[2], m),
            1 => self.draw_fs(f, chunks[2], m),
            2 => self.draw_mem(f, chunks[2], m),
            3 => self.draw_agents(f, chunks[2], m),
            _ => self.draw_usb(f, chunks[2]),
        }

        let footer = Paragraph::new(Line::from(vec![
            Span::styled("q", Style::default().fg(ratatui_amber())),
            Span::raw(" quit  "),
            Span::styled("Tab", Style::default().fg(ratatui_amber())),
            Span::raw(" views  "),
            Span::raw(format!(
                "peers {}  done {}  receipts {}",
                m.peers, m.completed_tasks, m.verified_receipts
            )),
        ]));
        f.render_widget(footer, chunks[3]);
    }

    fn draw_topology(&self, f: &mut ratatui::Frame, area: Rect, m: &ClusterMetrics) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(5)])
            .split(cols[0]);

        let cpu = Gauge::default()
            .block(Block::default().title("CPU").borders(Borders::ALL))
            .gauge_style(Style::default().fg(ratatui_cyan()))
            .percent(m.local_caps.cpu_util_pct.clamp(0.0, 100.0) as u16)
            .label(format!("{:.1}%", m.local_caps.cpu_util_pct));
        f.render_widget(cpu, left[0]);

        let mem_pct = if m.local_caps.mem_total_mb == 0 {
            0
        } else {
            (((m.local_caps.mem_total_mb - m.local_caps.mem_avail_mb) * 100)
                / m.local_caps.mem_total_mb) as u16
        };
        let mem = Gauge::default()
            .block(Block::default().title("Memory").borders(Borders::ALL))
            .gauge_style(Style::default().fg(ratatui_amber()))
            .percent(mem_pct.min(100))
            .label(format!(
                "{} / {} MiB",
                m.local_caps.mem_total_mb - m.local_caps.mem_avail_mb,
                m.local_caps.mem_total_mb
            ));
        f.render_widget(mem, left[1]);

        let spark = Sparkline::default()
            .block(Block::default().title("CPU history").borders(Borders::ALL))
            .style(Style::default().fg(ratatui_cyan()))
            .data(&self.cpu_hist);
        f.render_widget(spark, left[2]);

        let items = vec![
            ListItem::new(format!("Peers online: {}", m.peers)),
            ListItem::new(format!("Pending tasks: {}", m.pending_tasks)),
            ListItem::new(format!("Running tasks: {}", m.running_tasks)),
            ListItem::new(format!("Completed: {}", m.completed_tasks)),
            ListItem::new(format!("Load score: {:.2}", m.local_caps.load_score)),
            ListItem::new(format!("Pipeline R/W: {} / {}", m.bytes_read, m.bytes_written)),
        ];
        f.render_widget(
            List::new(items).block(Block::default().title("Cluster").borders(Borders::ALL)),
            cols[1],
        );
    }

    fn draw_fs(&self, f: &mut ratatui::Frame, area: Rect, m: &ClusterMetrics) {
        let hit_rate = if m.fs_cache_hits + m.fs_cache_misses == 0 {
            100.0
        } else {
            100.0 * m.fs_cache_hits as f32 / (m.fs_cache_hits + m.fs_cache_misses) as f32
        };
        let text = Paragraph::new(vec![
            Line::from(format!("CAS blocks stored: {}", m.fs_blocks)),
            Line::from(format!("Cache hits / misses: {} / {}", m.fs_cache_hits, m.fs_cache_misses)),
            Line::from(format!("Hit rate: {hit_rate:.1}%")),
            Line::from("DHT: gossip-indexed block→peer map"),
            Line::from("Mount: VirtualMount (/chimera/...) — FUSE optional on Unix"),
        ])
        .block(Block::default().title("ChimeraFS").borders(Borders::ALL).border_style(Style::default().fg(ratatui_cyan())));
        f.render_widget(text, area);
    }

    fn draw_mem(&self, f: &mut ratatui::Frame, area: Rect, m: &ClusterMetrics) {
        let text = Paragraph::new(vec![
            Line::from(format!("DSM regions: {}", m.mem_regions)),
            Line::from(format!("Local pages: {}", m.mem_local_pages)),
            Line::from(format!("Remote page faults: {}", m.mem_faults)),
            Line::from(format!("Wasm migrations: {}", m.migrations)),
            Line::from("Consistency: CRDT regions + optional ownership leases"),
            Line::from("Windows: soft page-table DSM (userfaultfd Linux-only feature)"),
        ])
        .block(Block::default().title("ChimeraMEM").borders(Borders::ALL).border_style(Style::default().fg(ratatui_amber())));
        f.render_widget(text, area);
    }

    fn draw_agents(&self, f: &mut ratatui::Frame, area: Rect, m: &ClusterMetrics) {
        let text = Paragraph::new(vec![
            Line::from(format!("Willingness: {:.2}", m.agent_willingness)),
            Line::from(format!("Healing pressure: {:.2}", m.agent_healing)),
            Line::from(format!("Intents compiled: {}", m.intents_compiled)),
            Line::from(format!("Verified receipts: {}", m.verified_receipts)),
            Line::from("Decision loop: telemetry ring → score → act (<1ms)"),
            Line::from("Economy: ed25519 + BLAKE3 receipts (zk optional)"),
        ])
        .block(Block::default().title("Agents & Economy").borders(Borders::ALL));
        f.render_widget(text, area);
    }

    fn draw_usb(&self, f: &mut ratatui::Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(3),
                Constraint::Min(4),
            ])
            .split(cols[0]);

        let fw = chimera_boot::firmware::detect_firmware_mode();
        let disks = chimera_boot::enumerate::list_disks().unwrap_or_default();
        let removable = disks.iter().filter(|d| d.removable).count();

        let layout = Paragraph::new(vec![
            Line::from("Partition layout (lab schematic)"),
            Line::from(" [MBR/GPT][======= FAT32 / ESP =======][ free ] "),
            Line::from(format!("Host firmware: {fw:?}")),
            Line::from(format!(
                "Disks visible: {} (removable={removable}) — list is READ-ONLY",
                disks.len()
            )),
        ])
        .block(
            Block::default()
                .title("USB Boot-Sovereign")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ratatui_amber())),
        );
        f.render_widget(layout, left[0]);

        let stage = (self.usb_tick / 4) % 4;
        let stages = ["enumerate", "partition", "stream-ISO", "verify-BLAKE3"];
        let pipe = stages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if i == stage as usize {
                    format!(">{s}<")
                } else {
                    (*s).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" → ");
        let gauge = Gauge::default()
            .block(Block::default().title("ISO pipeline").borders(Borders::ALL))
            .gauge_style(Style::default().fg(ratatui_cyan()))
            .percent(((stage + 1) * 25) as u16)
            .label(pipe);
        f.render_widget(gauge, left[1]);

        let spark = Sparkline::default()
            .block(Block::default().title("Write throughput (idle pulse)").borders(Borders::ALL))
            .style(Style::default().fg(ratatui_cyan()))
            .data(&self.usb_bps_hist);
        f.render_widget(spark, left[2]);

        let mut lines = vec![
            Line::from(Span::styled(
                "DATA LOSS RISK — dry-run ON by default",
                Style::default().fg(ratatui_amber()).add_modifier(Modifier::BOLD),
            )),
            Line::from("chimeractl usb list"),
            Line::from("chimeractl usb flash --image --target ./lab.img --payload ..."),
            Line::from("Physical flash: --no-dry-run --yes-i-understand-this-destroys-data"),
            Line::from("SMART media temp: unavailable (not fabricated)"),
            Line::from(""),
        ];
        for d in disks.iter().take(6) {
            lines.push(Line::from(format!(
                "{} {} rem={} sys={}",
                d.id,
                d.serial,
                d.removable,
                d.is_system
            )));
        }
        f.render_widget(
            Paragraph::new(lines).block(Block::default().title("Safety / disks").borders(Borders::ALL)),
            cols[1],
        );
    }
}

pub fn try_run(metrics: MetricsHub) -> anyhow::Result<()> {
    Dashboard::new(metrics).run().context("tui")
}
