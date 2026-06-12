use ratatui::{prelude::*, widgets::*, Frame};

use crate::helpers::{centered_rect_percent, find_all_subsequences, normalize_for_search};
use crate::keyboard::{Action, ActionCategory};
use crate::themes::theme::Theme;

use crokey::{KeyCombination, OneToThree};
use indexmap::IndexMap;
use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use strum::IntoEnumIterator;

pub fn render_help_modal(
    frame: &mut Frame,
    area: Rect,
    keymap: &IndexMap<KeyCombination, Action>,
    keymap_error: &Option<String>,
    scroll_state: &mut ScrollbarState,
    border_type: BorderType,
    theme: &Theme,
    search: &str,
    is_searching: bool,
    first_match: &mut Option<usize>,
) {
    let width_percent = area.width.clamp(30, 120) * 100 / area.width;
    let modal = centered_rect_percent(width_percent, 80, area);

    frame.render_widget(
        Block::default().style(
            Style::default()
                .bg(theme.resolve_opt(&theme.background).unwrap_or(Color::Reset)) // same bg
                .add_modifier(Modifier::DIM),
        ),
        area,
    );
    frame.render_widget(Clear, modal);

    let instructions = if is_searching || !search.is_empty() {
        Line::from(vec![
            Span::raw("Searching: "),
            Span::styled(search, Style::default().fg(theme.primary_color).bold()),
            Span::raw("  "),
            " Press <Esc> to exit search mode ".fg(theme.primary_color).bold(),
        ])
    } else {
        Line::from(vec![
            " Return ".fg(theme.resolve(&theme.foreground)),
            "<Esc> ".fg(theme.primary_color).bold(),
        ])
    };

    let block = Block::default()
        .title("Keymap")
        .title_bottom(instructions.alignment(Alignment::Center))
        .borders(Borders::ALL)
        .border_type(border_type)
        .style(Style::default().bg(theme.resolve_opt(&theme.background).unwrap_or(Color::Reset)))
        .border_style(theme.resolve(&theme.border_focused));

    frame.render_widget(block.clone(), modal);

    let inner = block.inner(modal);

    let header_height = if keymap_error.is_some() { 5 } else { 4 };

    let layout = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let header_area = layout[0];
    let separator_area = layout[1];
    let table_area = layout[2];

    let mut header_lines = vec![
        Line::from(""),
        Line::from("Active key bindings from your configuration")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.resolve(&theme.foreground))),
        Line::from("Changes reload automatically")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.primary_color)),
    ];

    if let Some(err) = keymap_error {
        header_lines.push(
            Line::from(vec![
                Span::styled(
                    "Error reading keymap:",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    err.strip_prefix("unknown variant `")
                        .and_then(|rest| rest.split_once('`'))
                        .map(|(variant, _)| format!("Unknown action: {}", variant))
                        .unwrap_or_else(|| err.to_string()),
                    Style::default().fg(theme.resolve(&theme.foreground)),
                ),
            ])
            .alignment(Alignment::Center),
        );
    }

    let header_text = Paragraph::new(header_lines);

    frame.render_widget(header_text, header_area);
    frame.render_widget(
        Block::default().borders(Borders::TOP).border_style(theme.resolve(&theme.border)),
        separator_area,
    );

    let mut grouped: IndexMap<ActionCategory, IndexMap<Action, Vec<KeyCombination>>> =
        IndexMap::new();

    for action in Action::iter().filter(|a| a.is_concrete()) {
        grouped.entry(action.category()).or_default().entry(action.clone()).or_default();
    }

    // second: populate actual bindings
    for (key, action) in keymap {
        grouped
            .entry(action.category())
            .or_default()
            .entry(action.clone())
            .or_default()
            .push(key.clone());
    }

    // Build a map from Action to its row index in the full (non-search) table layout.
    // This is needed so Cancel can scroll to the correct position after exiting search.
    let mut action_row: HashMap<Action, usize> = HashMap::new();
    {
        let mut idx = 1;
        for (_, actions) in &grouped {
            idx += 1;
            for (action, _) in actions {
                action_row.entry(action.clone()).or_insert(idx);
                idx += 1;
            }
            idx += 1;
        }
    }

    let search_active = !search.is_empty();
    let search_norm = normalize_for_search(search);

    let mut rows = Vec::new();
    rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]));

    *first_match = None;

    if search_active {
        let mut all_matches: Vec<(Action, Vec<KeyCombination>)> = Vec::new();
        for (_, actions) in grouped {
            for (action, keys) in actions {
                let key_str = if keys.is_empty() {
                    String::from("(unbound)")
                } else {
                    keys.iter().map(key_to_ui_string).collect::<Vec<_>>().join(", ")
                };
                let haystack =
                    format!("{} {} {}", key_str, action.to_config_string(), action.description());
                let haystack_norm = normalize_for_search(&haystack);
                if !find_all_subsequences(&search_norm, &haystack_norm).is_empty() {
                    // push found string to all_matches
                    all_matches.push((action, keys));
                }
            }
        }

        all_matches.sort_by_key(|(action, keys)| {
            let key_str = if keys.is_empty() {
                String::from("(unbound)")
            } else {
                keys.iter().map(key_to_ui_string).collect::<Vec<_>>().join(", ")
            };
            // get best match for search: Action -> Key binding -> Description
            // if user searches for 'reset', try matching Action first
            relevance_score(&search_norm, &key_str, action)
        });

        *first_match = all_matches.first().and_then(|(best, _)| action_row.get(best).copied());

        for (action, keys) in &all_matches {
            let key_str = if keys.is_empty() {
                String::from("(unbound)")
            } else {
                keys.iter().map(key_to_ui_string).collect::<Vec<_>>().join(", ")
            };

            let key_style = if keys.is_empty() {
                Style::default().fg(theme.resolve(&theme.foreground_dim)).bold()
            } else {
                Style::default().fg(theme.resolve(&theme.foreground)).bold()
            };

            rows.push(Row::new(vec![
                Cell::from(
                    highlighted_line(&key_str, &search_norm, key_style).alignment(Alignment::Right),
                ),
                {
                    let style = Style::default().fg(theme.resolve(&theme.foreground));
                    Cell::from(
                        highlighted_line(&action.to_config_string(), &search_norm, style)
                            .alignment(Alignment::Center),
                    )
                },
                {
                    let style = Style::default().fg(theme.resolve(&theme.foreground));
                    Cell::from(
                        highlighted_line(&action.description(), &search_norm, style)
                            .alignment(Alignment::Left),
                    )
                },
            ]));
        }
    } else {
        for (category, actions) in grouped {
            rows.push(
                Row::new(vec![
                    Cell::from(""),
                    Cell::from(Line::from(category.title()).alignment(Alignment::Center)),
                    Cell::from(""),
                ])
                .style(Style::default().fg(theme.primary_color).add_modifier(Modifier::BOLD)),
            );

            for (action, keys) in actions {
                let key_str = if keys.is_empty() {
                    String::from("(unbound)")
                } else {
                    keys.iter().map(key_to_ui_string).collect::<Vec<_>>().join(", ")
                };

                let key_style = if keys.is_empty() {
                    Style::default().fg(theme.resolve(&theme.foreground_dim)).bold()
                } else {
                    Style::default().fg(theme.resolve(&theme.foreground)).bold()
                };

                rows.push(Row::new(vec![
                    Cell::from(
                        highlighted_line(&key_str, &search_norm, key_style)
                            .alignment(Alignment::Right),
                    ),
                    {
                        let style = Style::default().fg(theme.resolve(&theme.foreground));
                        Cell::from(
                            highlighted_line(&action.to_config_string(), &search_norm, style)
                                .alignment(Alignment::Center),
                        )
                    },
                    {
                        let style = Style::default().fg(theme.resolve(&theme.foreground));
                        Cell::from(
                            highlighted_line(&action.description(), &search_norm, style)
                                .alignment(Alignment::Left),
                        )
                    },
                ]));
            }

            rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]));
        }
    }

    let total_rows = rows.len();
    let viewport = table_area.height.saturating_sub(1) as usize;
    let visible_rows = viewport.min(total_rows);
    let max_scroll = total_rows.saturating_sub(visible_rows);

    let position = scroll_state.get_position().min(max_scroll);

    *scroll_state =
        ScrollbarState::new(max_scroll).viewport_content_length(visible_rows).position(position);

    let header = Row::new(vec![
        Cell::from(Line::from("Key bindings").alignment(Alignment::Right)),
        Cell::from(Line::from("Action").alignment(Alignment::Center)),
        Cell::from(Line::from("Description").alignment(Alignment::Left)),
    ])
    .style(Style::default().fg(theme.resolve(&theme.section_title)).add_modifier(Modifier::BOLD));

    let visible = rows.into_iter().skip(position).take(visible_rows);
    let table = Table::new(
        visible,
        [Constraint::Percentage(35), Constraint::Percentage(30), Constraint::Percentage(35)],
    )
    .header(header)
    .column_spacing(2);

    frame.render_widget(table, table_area);
    crate::helpers::render_scrollbar(frame, table_area, scroll_state, theme);
}

fn highlighted_line(text: &str, search_norm: &str, style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    let mut last_end = 0;
    let all_subsequences = find_all_subsequences(search_norm, &text.to_lowercase());

    for (start, end) in all_subsequences {
        if last_end < start {
            spans.push(Span::styled(text[last_end..start].to_string(), style));
        }

        spans.push(Span::styled(text[start..end].to_string(), style.underlined()));

        last_end = end;
    }

    if last_end < text.len() {
        spans.push(Span::styled(text[last_end..].to_string(), style));
    }

    Line::from(spans)
}

fn key_to_ui_string(key: &KeyCombination) -> String {
    let mut s = String::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        s.push_str("Ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        s.push_str("Alt-");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        s.push_str("Shift-");
    }

    match key.codes {
        OneToThree::One(KeyCode::Char(c)) => {
            if c == ' ' {
                s = "Space".to_string();
            } else {
                s.push(c.to_ascii_lowercase());
            }
        }
        OneToThree::One(code) => {
            // use crokey naming for special keys only
            let tmp = KeyCombination::one_key(code, KeyModifiers::empty());
            s.push_str(&tmp.to_string());
        }
        _ => {
            s.push_str(&key.to_string());
        }
    }

    s
}

/// Helper function for handling partial match vs full match
/// Matches Action -> Keybindings / Description
fn relevance_score(search_norm: &str, key_str: &str, action: &Action) -> (u8, usize) {
    let action_norm = normalize_for_search(&action.to_config_string());
    if let Some(off) = find_all_subsequences(search_norm, &action_norm).first().map(|(s, _)| *s) {
        // search matches Action, return as best match
        return (0, off);
    }

    let desc_norm = normalize_for_search(&action.description());
    let name_desc_norm = format!("{} {}", action_norm, desc_norm);
    if let Some(off) = find_all_subsequences(search_norm, &name_desc_norm).first().map(|(s, _)| *s)
    {
        return (1, off);
    }

    let full_norm = format!("{} {}", key_str, name_desc_norm);
    let off = find_all_subsequences(search_norm, &full_norm)
        .first()
        .map(|(s, _)| *s)
        .unwrap_or(usize::MAX);
    (2, off)
}

pub fn build_tab_labels(keymap: &IndexMap<KeyCombination, Action>) -> [String; 4] {
    let names = ["Library", "Albums", "Playlists", "Search"];

    std::array::from_fn(|i| {
        let action = Action::Tab((i + 1) as u8);

        let binding = keymap
            .iter()
            .find(|(_, a)| **a == action)
            .map(|(k, _)| key_to_ui_string(k))
            .unwrap_or_else(|| (i + 1).to_string());

        format!("{}: {}", binding, names[i])
    })
}
