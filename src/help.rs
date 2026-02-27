use ratatui::{prelude::*, widgets::*, Frame};

use crate::helpers::centered_rect_percent;
use crate::keyboard::{Action, ActionCategory};
use crate::themes::theme::Theme;

use crokey::{KeyCombination, OneToThree};
use indexmap::IndexMap;

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

    let instructions = Line::from(vec![
        " Return ".fg(theme.resolve(&theme.foreground)),
        "<Esc> ".fg(theme.primary_color).bold(),
    ]);

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

    let mut rows = Vec::new();
    rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]));

    for (category, actions) in grouped {
        // category header
        rows.push(
            Row::new(vec![
                Cell::from(""),
                Cell::from(Line::from(category.title()).alignment(Alignment::Center)),
                Cell::from(""),
            ])
            .style(Style::default().fg(theme.primary_color).add_modifier(Modifier::BOLD)),
        );

        for (action, keys) in actions {
            let key_cell = match keys.len() {
                0 => Cell::from(Line::from(String::from("(unbound)")).alignment(Alignment::Right))
                    .style(Style::default().fg(theme.resolve(&theme.foreground_dim)).bold()),
                _ => Cell::from(
                    Line::from(keys.iter().map(key_to_ui_string).collect::<Vec<_>>().join(", "))
                        .alignment(Alignment::Right)
                        .style(Style::default().fg(theme.resolve(&theme.foreground)).bold()),
                ),
            };
            rows.push(Row::new(vec![
                key_cell,
                Cell::from(Line::from(action.to_config_string()).alignment(Alignment::Center))
                    .style(Style::default().fg(theme.resolve(&theme.foreground))),
                Cell::from(Line::from(action.description()).alignment(Alignment::Left))
                    .style(Style::default().fg(theme.resolve(&theme.foreground))),
            ]));
        }

        // spacer row
        rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]));
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
