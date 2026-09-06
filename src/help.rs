use ratatui::{prelude::*, widgets::*, Frame};

use crate::helpers::{
    centered_rect_percent, find_all_subsequences, normalize_for_search, search_ranked_indices,
    Searchable,
};
use crate::keyboard::{Action, ActionCategory};
use crate::themes::theme::Theme;

use crokey::{KeyCombination, OneToThree};
use indexmap::IndexMap;

use crossterm::event::{KeyCode, KeyModifiers};
use strum::IntoEnumIterator;

struct ActionEntry {
    action: Action,
    keys: Vec<KeyCombination>,
    search_name: String,
}
impl Searchable for ActionEntry {
    fn id(&self) -> &str {
        &self.search_name
    }
    fn name(&self) -> &str {
        &self.search_name
    }
}

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
) {
    let width_percent = area.width.clamp(30, 120) * 100 / area.width;
    let modal = centered_rect_percent(width_percent, 80, area);

    frame.render_widget(
        Block::default().style(
            Style::default()
                .bg(theme.resolve_opt(&theme.background).unwrap_or(Color::Reset))
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
            " Exit search ".fg(theme.resolve(&theme.foreground)),
            "<Esc> ".fg(theme.primary_color).bold(),
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
    frame.render_widget(Paragraph::new(header_lines), layout[0]);
    frame.render_widget(
        Block::default().borders(Borders::TOP).border_style(theme.resolve(&theme.border)),
        layout[1],
    );

    let table_area = layout[2];
    let grouped = build_grouped(keymap);
    let search_norm = normalize_for_search(search);
    let mut rows: Vec<Row> = vec![Row::new(["", "", ""])];

    if search.is_empty() {
        for (category, actions) in &grouped {
            rows.push(
                Row::new(vec![
                    Cell::from(""),
                    Cell::from(Line::from(category.title()).alignment(Alignment::Center)),
                    Cell::from(""),
                ])
                .style(Style::default().fg(theme.primary_color).add_modifier(Modifier::BOLD)),
            );
            for (action, keys) in actions {
                rows.push(make_row(action, keys, "", theme));
            }
            rows.push(Row::new(["", "", ""]));
        }
    } else {
        let entries: Vec<ActionEntry> = grouped
            .into_values()
            .flat_map(|a| a.into_iter())
            .map(|(action, keys)| ActionEntry {
                search_name: format!(
                    "{} {} {}",
                    action.to_config_string(),
                    action.description(),
                    key_combo_string(&keys)
                ),
                action,
                keys,
            })
            .collect();
        for i in search_ranked_indices(&entries, search, false) {
            rows.push(make_row(&entries[i].action, &entries[i].keys, &search_norm, theme));
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

    frame.render_widget(
        Table::new(
            rows.into_iter().skip(position).take(visible_rows),
            [Constraint::Percentage(35), Constraint::Percentage(30), Constraint::Percentage(35)],
        )
        .header(header)
        .column_spacing(2),
        table_area,
    );
    crate::helpers::render_scrollbar(frame, table_area, scroll_state, theme);
}

fn build_grouped(
    keymap: &IndexMap<KeyCombination, Action>,
) -> IndexMap<ActionCategory, IndexMap<Action, Vec<KeyCombination>>> {
    let mut grouped: IndexMap<ActionCategory, IndexMap<Action, Vec<KeyCombination>>> =
        IndexMap::new();
    for action in Action::iter().filter(|a| a.is_concrete()) {
        grouped.entry(action.category()).or_default().entry(action.clone()).or_default();
    }
    for (key, action) in keymap {
        grouped
            .entry(action.category())
            .or_default()
            .entry(action.clone())
            .or_default()
            .push(key.clone());
    }
    grouped
}

fn key_combo_string(keys: &[KeyCombination]) -> String {
    if keys.is_empty() {
        "(unbound)".to_string()
    } else {
        keys.iter().map(key_to_ui_string).collect::<Vec<_>>().join(", ")
    }
}

fn make_row<'a>(
    action: &Action,
    keys: &[KeyCombination],
    search_norm: &str,
    theme: &Theme,
) -> Row<'a> {
    let key_str = key_combo_string(keys);
    let key_style = if keys.is_empty() {
        Style::default().fg(theme.resolve(&theme.foreground_dim)).bold()
    } else {
        Style::default().fg(theme.resolve(&theme.foreground)).bold()
    };
    let fg = Style::default().fg(theme.resolve(&theme.foreground));
    Row::new(vec![
        Cell::from(underline_matches(&key_str, search_norm, key_style).alignment(Alignment::Right)),
        Cell::from(
            underline_matches(&action.to_config_string(), search_norm, fg)
                .alignment(Alignment::Center),
        ),
        Cell::from(
            underline_matches(&action.description(), search_norm, fg).alignment(Alignment::Left),
        ),
    ])
}

fn underline_matches(text: &str, search_norm: &str, style: Style) -> Line<'static> {
    if search_norm.is_empty() {
        return Line::from(Span::styled(text.to_string(), style));
    }
    let lower = text.to_lowercase();
    let mut spans = Vec::new();
    let mut last = 0;
    for (start, end) in find_all_subsequences(search_norm, &lower) {
        if last < start {
            spans.push(Span::styled(text[last..start].to_string(), style));
        }
        spans.push(Span::styled(text[start..end].to_string(), style.underlined()));
        last = end;
    }
    if last < text.len() {
        spans.push(Span::styled(text[last..].to_string(), style));
    }
    Line::from(spans)
}

/// The key currently bound to `action`, formatted for an instruction footer.
/// Returns `fallback` when the user has left the action unbound.
pub fn key_hint(
    keymap: &IndexMap<KeyCombination, Action>,
    action: &Action,
    fallback: &str,
) -> String {
    key_hint_by(keymap, |a| a == action, fallback)
}

/// Same, for actions carrying a payload (Seek, Volume, Jump, ...) where the
/// caller only cares about the direction.
pub fn key_hint_by(
    keymap: &IndexMap<KeyCombination, Action>,
    predicate: impl Fn(&Action) -> bool,
    fallback: &str,
) -> String {
    keymap
        .iter()
        .find(|(_, a)| predicate(a))
        .map(|(k, _)| format!("<{}>", key_to_ui_string(k)))
        .unwrap_or_else(|| fallback.to_string())
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
            let tmp = KeyCombination::one_key(code, KeyModifiers::empty());
            s.push_str(&tmp.to_string());
        }
        _ => s.push_str(&key.to_string()),
    }
    s
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
