use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, View};
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(false))
        .title(Span::styled(" k8s-tui ", theme::title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cluster = app.backend.cluster();
    let line1 = Line::from(vec![
        Span::styled("cluster ", theme::header_label()),
        Span::styled(cluster.name.clone(), theme::header_value()),
        Span::styled("  provider ", theme::header_label()),
        Span::styled(cluster.provider.label(), theme::header_value()),
        Span::styled("  context ", theme::header_label()),
        Span::styled(cluster.context.clone(), theme::header_value()),
        Span::styled("  k8s ", theme::header_label()),
        Span::styled(cluster.k8s_version.clone(), theme::header_value()),
        Span::styled("  nodes ", theme::header_label()),
        Span::styled(cluster.node_count.to_string(), theme::header_value()),
    ]);

    let line2 = match app.current_view() {
        View::Clusters => Line::from(vec![Span::styled(
            format!("{} clusters available — pick one and press Enter", app.backend.clusters().len()),
            theme::dim(),
        )]),
        View::Table(kind) => {
            let count = app.visible_rows(kind).len();
            let mut spans = vec![
                Span::styled(kind.title(), theme::header_value()),
                Span::styled(format!("  ({count})"), theme::dim()),
            ];
            if kind.namespaced() {
                spans.push(Span::styled("  ns ", theme::header_label()));
                spans.push(Span::styled(app.namespace_label().to_string(), theme::header_value()));
            }
            if !app.filter.is_empty() {
                spans.push(Span::styled("  filter ", theme::header_label()));
                spans.push(Span::styled(format!("/{}", app.filter), theme::header_value()));
            }
            Line::from(spans)
        }
        View::Detail { kind, name, .. } => Line::from(vec![
            Span::styled(format!("{} › ", kind.title()), theme::dim()),
            Span::styled(name, theme::header_value()),
        ]),
        View::Logs { pod, .. } => Line::from(vec![
            Span::styled("logs › ", theme::dim()),
            Span::styled(pod, theme::header_value()),
        ]),
    };

    let (cpu, mem) = app.backend.cluster_usage();
    let line3 = Line::from(vec![
        Span::styled("CPU ", theme::header_label()),
        Span::styled(bar(cpu, 20), gauge_style(cpu)),
        Span::styled(format!(" {cpu:>3}%  "), theme::dim()),
        Span::styled("MEM ", theme::header_label()),
        Span::styled(bar(mem, 20), gauge_style(mem)),
        Span::styled(format!(" {mem:>3}%"), theme::dim()),
    ]);

    let p = Paragraph::new(vec![line1, line2, line3]);
    f.render_widget(p, inner);
}

fn bar(pct: u8, width: usize) -> String {
    let filled = ((pct as usize) * width / 100).min(width);
    let mut s = String::with_capacity(width);
    for i in 0..width {
        s.push(if i < filled { '█' } else { '░' });
    }
    s
}

fn gauge_style(pct: u8) -> ratatui::style::Style {
    if pct >= 85 {
        ratatui::style::Style::default().fg(theme::BAD)
    } else if pct >= 65 {
        ratatui::style::Style::default().fg(theme::WARN)
    } else {
        ratatui::style::Style::default().fg(theme::FG_DIM)
    }
}
