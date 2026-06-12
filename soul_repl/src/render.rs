//! Méthodes de rendu TUI (draw methods).

use crate::types::*;
use crate::utils::*;
use crate::App;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use std::fs;

impl App {
    pub(crate) fn draw(&mut self, f: &mut Frame) {
        let size = f.area();

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(3)])
            .split(size);

        self.draw_status_bar(f, vertical[0]);
        self.draw_chat(f, vertical[1]);
        self.draw_input(f, vertical[2]);
        self.draw_toasts(f, size);

        match self.focus {
            Focus::ModelDialog => self.draw_model_dialog(f, size),
            Focus::ProviderDialog => self.draw_provider_dialog(f, size),
            Focus::HelpDialog => self.draw_help_dialog(f, size),
            Focus::CommandPalette => self.draw_command_palette(f, size),
            Focus::FileBrowser => self.draw_file_browser(f, size),
            Focus::HistorySearch => self.draw_history_search(f, size),
            Focus::SessionManager => self.draw_session_manager(f, size),
            _ => {}
        }
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let provider_label = self.provider_items.get(self.provider_selected).map(|p| p.label()).unwrap_or("?");
        let elapsed = self.last_input_time.elapsed().as_secs();

        let status_line = Line::from(vec![
            Span::styled(" SoulSystem ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", provider_label), Style::default().fg(Color::White).bg(Color::DarkGray)),
            Span::styled(format!(" {} ", self.model_items.first().unwrap_or(&String::new())), Style::default().fg(Color::Green)),
            Span::raw(" | "),
            Span::styled(format!("{} msgs", self.messages.len()), Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled(format!("{} cmds", self.command_count), Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled(
                if self.streaming { "streaming" }
                else if elapsed < 5 { "ready" }
                else { "idle" },
                Style::default().fg(if self.streaming { Color::Yellow } else { Color::Green })
            ),
            Span::raw(" | "),
            Span::styled("Ctrl+Shift+P palette", Style::default().fg(Color::DarkGray)),
        ]);
        let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)).style(Style::default().bg(Color::Black));
        f.render_widget(Paragraph::new(status_line).block(block).style(Style::default().bg(Color::Black)), area);
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Chat ").border_style(Style::default().fg(Color::DarkGray)).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        for msg in &self.messages {
            let (prefix, style) = match msg.role {
                Role::User => ("you", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Role::Assistant => ("ai", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Role::System => ("sys", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Role::Error => ("err", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Role::Tool => ("tool", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", prefix), style),
                Span::styled(&msg.text, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::raw(""));
        }

        if self.streaming && !self.stream_buffer.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("ai ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(&self.stream_buffer, Style::default().fg(Color::White)),
            ]));
        }

        let paragraph = Paragraph::new(lines).scroll((self.scroll_offset, 0)).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Input;
        let block = Block::default().borders(Borders::ALL).title(" Input (Shift+Enter multi-ligne) ").border_style(Style::default().fg(if focused { Color::Cyan } else { Color::DarkGray })).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let display = if self.input.is_empty() && focused {
            Span::styled("Tapez votre message...", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(&self.input, Style::default().fg(Color::White))
        };

        f.render_widget(Paragraph::new(Line::from(display)), inner);

        if focused {
            f.set_cursor_position((inner.x + self.input_cursor as u16, inner.y));
        }
    }

    fn draw_toasts(&self, f: &mut Frame, size: Rect) {
        let max_y = size.height.saturating_sub(4);
        for (y, toast) in (4u16..max_y).zip(self.toasts.iter()) {
            let toast_area = Rect {
                x: size.width.saturating_sub(40),
                y,
                width: 38,
                height: 1,
            };
            let line = Line::from(vec![
                Span::styled(format!(" {} ", toast.level.icon()), Style::default().fg(toast.level.color()).add_modifier(Modifier::BOLD)),
                Span::styled(&toast.message, Style::default().fg(Color::White)),
            ]);
            f.render_widget(Paragraph::new(line).style(Style::default().bg(Color::Black)), toast_area);
        }
    }

    fn draw_model_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(50, 40, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Selectionner un modele ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let items: Vec<ListItem> = self.model_items.iter().enumerate().map(|(i, m)| {
            let style = if i == self.model_selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(m.as_str(), style)))
        }).collect();
        let list = List::new(items).block(block).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.model_selected));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_provider_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(50, 30, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Selectionner un provider ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let items: Vec<ListItem> = self.provider_items.iter().enumerate().map(|(i, p)| {
            let style = if i == self.provider_selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(p.label(), style)))
        }).collect();
        let list = List::new(items).block(block).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.provider_selected));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_help_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(70, 80, size);
        f.render_widget(Clear, area);
        let help_text = vec![
            Line::from(Span::styled(" SoulSystem REPL - Aide", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::raw(""),
            Line::from(Span::styled(" Commandes:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::raw("   ask <msg>        Poser une question au LLM"),
            Line::raw("   plan <objectif>  Creer un plan"),
            Line::raw("   run <cmd>        Executer une commande"),
            Line::raw("   models           Lister les modeles"),
            Line::raw("   status           Etat du systeme"),
            Line::raw("   clear            Effacer l'historique"),
            Line::raw("   save             Sauvegarder la session"),
            Line::raw("   export           Exporter le chat en MD"),
            Line::raw("   files            Navigateur de fichiers"),
            Line::raw("   search <query>   Rechercher dans l'historique"),
            Line::raw("   help             Cette aide"),
            Line::raw("   exit             Quitter"),
            Line::raw(""),
            Line::from(Span::styled(" Raccourcis:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::raw("   Ctrl+C             Quitter"),
            Line::raw("   Ctrl+Shift+P       Palette de commandes"),
            Line::raw("   Ctrl+P             Changer provider"),
            Line::raw("   Ctrl+M             Changer modele"),
            Line::raw("   Ctrl+H             Aide"),
            Line::raw("   Ctrl+F             Navigateur de fichiers"),
            Line::raw("   Ctrl+R             Recherche historique"),
            Line::raw("   Ctrl+S             Sauvegarder session"),
            Line::raw("   Ctrl+O             Charger session"),
            Line::raw("   Ctrl+Y             Copier derniere reponse"),
            Line::raw("   Shift+Enter        Nouvelle ligne"),
            Line::raw("   Tab                Autocomplete"),
            Line::raw("   Esc                Fermer dialog"),
            Line::raw(""),
            Line::from(Span::styled(" Esc pour fermer", Style::default().fg(Color::DarkGray))),
        ];
        let block = Block::default().borders(Borders::ALL).title(" Aide ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        f.render_widget(Paragraph::new(help_text).block(block).wrap(Wrap { trim: false }), area);
    }

    fn draw_command_palette(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(60, 60, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Command Palette (Ctrl+Shift+P) ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let filter_line = Line::from(vec![
            Span::styled(" | ", Style::default().fg(Color::Cyan)),
            Span::styled(&self.command_palette_filter, Style::default().fg(Color::White)),
        ]);
        let filter_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
        f.render_widget(Paragraph::new(filter_line), filter_area);

        let items = self.filtered_command_palette_items();
        let list_area = Rect { x: inner.x, y: inner.y + 2, width: inner.width, height: inner.height.saturating_sub(2) };

        let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, item)| {
            let style = if i == self.command_palette_selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
            let shortcut = item.shortcut.as_deref().unwrap_or("");
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {:20}", item.name), style),
                Span::styled(format!("{:30}", item.description), Style::default().fg(Color::DarkGray)),
                Span::styled(shortcut, Style::default().fg(Color::Yellow)),
            ]))
        }).collect();

        let list = List::new(list_items).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.command_palette_selected));
        f.render_stateful_widget(list, list_area, &mut state);
    }

    fn draw_file_browser(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(70, 70, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(format!(" {} ", self.file_browser_path.display())).border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let horizontal = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

        let items: Vec<ListItem> = self.file_browser_entries.iter().skip(self.file_browser_scroll).take(20).enumerate().map(|(i, entry)| {
            let actual_idx = i + self.file_browser_scroll;
            let style = if actual_idx == self.file_browser_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if entry.is_dir {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::White)
            };
            let icon = if entry.is_dir { "D" } else { "F" };
            let size_str = if entry.is_dir { String::new() } else { format!(" ({})", format_size(entry.size)) };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} {}", icon, entry.name), style),
                Span::styled(size_str, Style::default().fg(Color::DarkGray)),
            ]))
        }).collect();

        let list = List::new(items).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.file_browser_selected.saturating_sub(self.file_browser_scroll)));
        f.render_stateful_widget(list, horizontal[0], &mut state);

        if let Some(entry) = self.file_browser_entries.get(self.file_browser_selected) {
            if !entry.is_dir {
                if let Ok(content) = fs::read_to_string(&entry.path) {
                    let preview: Vec<Line> = content.lines().take(20).map(|l| {
                        Line::from(Span::styled(l, Style::default().fg(Color::White)))
                    }).collect();
                    let block = Block::default().title(" Preview ").border_style(Style::default().fg(Color::DarkGray));
                    f.render_widget(Paragraph::new(preview).block(block).wrap(Wrap { trim: false }), horizontal[1]);
                }
            }
        }
    }

    fn draw_history_search(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(60, 50, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Recherche historique (Ctrl+R) ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if let Some(ref search) = self.history_search {
            let search_line = Line::from(vec![
                Span::styled(" | ", Style::default().fg(Color::Cyan)),
                Span::styled(&search.query, Style::default().fg(Color::White)),
            ]);
            let search_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
            f.render_widget(Paragraph::new(search_line), search_area);

            let results_area = Rect { x: inner.x, y: inner.y + 2, width: inner.width, height: inner.height.saturating_sub(2) };
            let results: Vec<ListItem> = search.results.iter().enumerate().map(|(i, &msg_idx)| {
                let msg = &self.messages[msg_idx];
                let style = if i == search.selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
                let role = match msg.role {
                    Role::User => "you",
                    Role::Assistant => "ai",
                    Role::System => "sys",
                    Role::Error => "err",
                    Role::Tool => "tool",
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  [{}] ", role), Style::default().fg(Color::DarkGray)),
                    Span::styled(&msg.text[..msg.text.len().min(50)], style),
                ]))
            }).collect();

            let list = List::new(results).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
            let mut state = ListState::default();
            state.select(Some(search.selected));
            f.render_stateful_widget(list, results_area, &mut state);
        }
    }

    fn draw_session_manager(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(60, 50, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Sessions ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.sessions.is_empty() {
            let empty = Paragraph::new("Aucune session sauvegardee.\nUtilisez Ctrl+S pour sauvegarder.").style(Style::default().fg(Color::DarkGray));
            f.render_widget(empty, inner);
            return;
        }

        let items: Vec<ListItem> = self.sessions.iter().enumerate().map(|(i, session)| {
            let style = if i == self.session_selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {}", session.name), style),
                Span::styled(format!(" ({} msgs)", session.message_count), Style::default().fg(Color::DarkGray)),
            ]))
        }).collect();

        let list = List::new(items).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.session_selected));
        f.render_stateful_widget(list, inner, &mut state);
    }

    #[allow(dead_code)]
    fn draw_diff_viewer(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(80, 80, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Diff ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let paragraph = Paragraph::new(self.diff_content.clone()).block(block).wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }
}
