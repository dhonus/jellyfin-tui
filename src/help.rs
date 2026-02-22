use ratatui::{prelude::*, widgets::*, Frame};

use crate::helpers::centered_rect_percent;
use crate::keyboard::{Action, ActionCategory};
use crate::themes::theme::Theme;

use crokey::KeyCombination;
use indexmap::IndexMap;

pub fn render_help_modal(
    frame: &mut Frame,
    area: Rect,
    keymap: &IndexMap<KeyCombination, Action>,
    scroll_state: &mut ScrollbarState,
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
        .style(Style::default().bg(theme.resolve_opt(&theme.background).unwrap_or(Color::Reset)))
        .border_style(theme.resolve(&theme.border_focused));

    frame.render_widget(block.clone(), modal);

    let inner = block.inner(modal);

    let mut grouped: IndexMap<ActionCategory, IndexMap<Action, Vec<KeyCombination>>> =
        IndexMap::new();

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
            let keys_str =
                keys.iter().map(KeyCombination::to_string).collect::<Vec<_>>().join(", ");

            rows.push(Row::new(vec![
                Cell::from(Line::from(keys_str.clone()).alignment(Alignment::Right))
                    .style(Style::default().fg(theme.resolve(&theme.foreground)).bold()),
                Cell::from(Line::from(format!("{:?}", action)).alignment(Alignment::Center))
                    .style(Style::default().fg(theme.resolve(&theme.foreground))),
                Cell::from(Line::from(action.description()).alignment(Alignment::Left))
                    .style(Style::default().fg(theme.resolve(&theme.foreground))),
            ]));
        }

        // spacer row
        rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]));
    }

    let total_rows = rows.len();
    let viewport = inner.height as usize;
    let visible_rows = viewport.min(total_rows);
    let max_scroll = total_rows.saturating_sub(visible_rows);

    let position = scroll_state.get_position().min(max_scroll);

    *scroll_state =
        ScrollbarState::new(max_scroll).viewport_content_length(visible_rows).position(position);

    let header = Row::new(vec![
        Cell::from(Line::from("Keys").alignment(Alignment::Right)),
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

    frame.render_widget(table, inner);

    crate::helpers::render_scrollbar(frame, inner, scroll_state, theme);
}
