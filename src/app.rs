use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::{ListState, TableState};

use crate::{
    daemon::daemon::Daemon,
    data::{MockBackend, ResourceKind, ResourceRow, pod_row},
};

const DATA_TICK: Duration = Duration::from_secs(2);
const LOG_TICK: Duration = Duration::from_millis(450);
const ALL_NAMESPACES: &str = "<all>";

#[derive(Clone)]
pub enum View {
    Clusters,
    Table(ResourceKind),
    Detail {
        kind: ResourceKind,
        namespace: Option<String>,
        name: String,
    },
    Logs {
        namespace: String,
        pod: String,
    },
}

#[derive(PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Filter,
    CreateName,
    ConfirmDelete,
    Edit,
}

pub struct App {
    pub backend: MockBackend,
    pub daemon: Daemon,
    pub view_stack: Vec<View>,
    pub mode: Mode,
    pub input: String,
    pub filter: String,
    pub namespace_filter: Option<String>,
    pub table_state: TableState,
    pub cluster_state: ListState,
    pub log_lines: Vec<String>,
    log_seq: u64,
    log_container: String,
    pub help_visible: bool,
    pub should_quit: bool,
    pub status_message: Option<String>,
    create_kind: Option<ResourceKind>,
    pending_delete: Option<(ResourceKind, Option<String>, String)>,
    edit_target: Option<(ResourceKind, Option<String>, String)>,
    last_data_tick: Instant,
    last_log_tick: Instant,
}

impl App {
    pub fn new(daemon: Daemon) -> Self {
        let mut cluster_state = ListState::default();
        cluster_state.select(Some(0));
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        App {
            backend: MockBackend::new(),
            daemon,
            view_stack: vec![View::Clusters],
            mode: Mode::Normal,
            input: String::new(),
            filter: String::new(),
            namespace_filter: None,
            table_state,
            cluster_state,
            log_lines: Vec::new(),
            log_seq: 0,
            log_container: String::new(),
            help_visible: false,
            should_quit: false,
            status_message: None,
            create_kind: None,
            pending_delete: None,
            edit_target: None,
            last_data_tick: Instant::now(),
            last_log_tick: Instant::now(),
        }
    }

    pub fn current_view(&self) -> View {
        self.view_stack.last().cloned().unwrap_or(View::Clusters)
    }

    fn push_view(&mut self, view: View) {
        self.view_stack.push(view);
        self.table_state.select(Some(0));
    }

    fn pop_view(&mut self) {
        if self.view_stack.len() > 1 {
            self.view_stack.pop();
            self.filter.clear();
            self.table_state.select(Some(0));
        }
    }

    fn goto_clusters(&mut self) {
        self.view_stack = vec![View::Clusters];
        self.namespace_filter = None;
        self.filter.clear();
        self.table_state.select(Some(0));
    }

    fn switch_kind(&mut self, kind: ResourceKind) {
        match self.view_stack.last_mut() {
            Some(View::Table(k)) => *k = kind,
            _ => self.view_stack.push(View::Table(kind)),
        }
        self.namespace_filter = None;
        self.filter.clear();
        self.table_state.select(Some(0));
    }

    pub fn visible_rows(&self, kind: ResourceKind) -> Vec<ResourceRow> {
        let ns = self.namespace_filter.as_deref();
        let mut rows = if kind == ResourceKind::Pods {
            let mut pods = self.daemon.pods();
            if let Some(ns_filter) = ns {
                pods.retain(|p| p.namespace == ns_filter);
            }
            pods.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
            pods.iter().map(pod_row).collect()
        } else {
            self.backend.rows(kind, ns)
        };
        if !self.filter.is_empty() {
            let needle = self.filter.to_ascii_lowercase();
            rows.retain(|r| {
                r.cells
                    .iter()
                    .any(|c| c.to_ascii_lowercase().contains(&needle))
            });
        }
        rows
    }

    pub fn on_tick(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_data_tick) >= DATA_TICK {
            self.backend.tick();
            self.last_data_tick = now;
        }
        if let View::Logs { .. } = self.current_view() {
            if now.duration_since(self.last_log_tick) >= LOG_TICK {
                self.log_seq += 1;
                let line = self
                    .backend
                    .next_log_line(&self.log_container, self.log_seq);
                self.log_lines.push(line);
                if self.log_lines.len() > 500 {
                    let excess = self.log_lines.len() - 500;
                    self.log_lines.drain(0..excess);
                }
                self.last_log_tick = now;
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match self.mode {
            Mode::Command => self.on_key_command(key),
            Mode::Filter => self.on_key_filter(key),
            Mode::CreateName => self.on_key_create(key),
            Mode::ConfirmDelete => self.on_key_confirm_delete(key),
            Mode::Edit => self.on_key_edit(key),
            Mode::Normal => self.on_key_normal(key),
        }
    }

    fn on_key_edit(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
                self.edit_target = None;
            }
            KeyCode::Enter => {
                if let Some((kind, namespace, name)) = self.edit_target.take() {
                    let value = self.input.clone();
                    match self
                        .backend
                        .apply_edit(kind, namespace.as_deref(), &name, &value)
                    {
                        Ok(()) => {
                            self.status_message = Some(format!(
                                "updated {} '{}' {}",
                                kind.title(),
                                name,
                                kind.edit_field_label()
                            ))
                        }
                        Err(e) => self.status_message = Some(format!("error: {e}")),
                    }
                }
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn on_key_create(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
                self.create_kind = None;
            }
            KeyCode::Enter => {
                if let Some(kind) = self.create_kind.take() {
                    let ns = self
                        .namespace_filter
                        .clone()
                        .unwrap_or_else(|| "default".to_string());
                    let name = self.input.trim().to_string();
                    match self.backend.create_default(kind, Some(&ns), &name) {
                        Ok(()) => {
                            self.status_message =
                                Some(format!("created {} '{}'", kind.title(), name))
                        }
                        Err(e) => self.status_message = Some(format!("error: {e}")),
                    }
                }
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn on_key_confirm_delete(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some((kind, namespace, name)) = self.pending_delete.take() {
                    match self.backend.delete(kind, namespace.as_deref(), &name) {
                        Ok(()) => {
                            self.status_message =
                                Some(format!("deleted {} '{}'", kind.title(), name));
                            self.move_selection(0);
                        }
                        Err(e) => self.status_message = Some(format!("error: {e}")),
                    }
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.pending_delete = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn on_key_command(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                self.execute_command();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn on_key_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                self.filter = self.input.clone();
                self.mode = Mode::Normal;
                self.table_state.select(Some(0));
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn execute_command(&mut self) {
        let cmd = self.input.trim().to_string();
        self.input.clear();
        if cmd.is_empty() {
            return;
        }
        match cmd.as_str() {
            "q" | "quit" => self.should_quit = true,
            "ctx" | "cluster" | "clusters" => self.goto_clusters(),
            _ => {
                if let Some(kind) = ResourceKind::from_alias(&cmd) {
                    if matches!(self.current_view(), View::Clusters) {
                        self.push_view(View::Table(kind));
                    } else {
                        self.switch_kind(kind);
                    }
                    self.status_message = None;
                } else {
                    self.status_message = Some(format!("unknown command: {}", cmd));
                }
            }
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent) {
        if self.help_visible {
            if matches!(
                key.code,
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')
            ) {
                self.help_visible = false;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.help_visible = true,
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.input.clear();
            }
            KeyCode::Char('/') => {
                if matches!(self.current_view(), View::Table(_)) {
                    self.mode = Mode::Filter;
                    self.input = self.filter.clone();
                }
            }
            KeyCode::Esc => {
                if !self.filter.is_empty() && matches!(self.current_view(), View::Table(_)) {
                    self.filter.clear();
                    self.table_state.select(Some(0));
                } else {
                    self.pop_view();
                }
            }
            KeyCode::Char('c') => self.goto_clusters(),
            KeyCode::Char('r') => self.backend.tick(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Char('g') => self.select_index(0),
            KeyCode::Char('G') => self.select_last(),
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_create()
            }
            KeyCode::Char('n') => self.cycle_namespace(),
            KeyCode::Char('[') => self.cycle_kind(-1),
            KeyCode::Char(']') => self.cycle_kind(1),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_delete_confirm()
            }
            KeyCode::Enter | KeyCode::Char('d') => self.drill_in(),
            KeyCode::Char('l') => self.open_logs(),
            KeyCode::Char('e') => self.start_edit(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        match self.current_view() {
            View::Clusters => {
                let len = self.backend.clusters.len();
                let cur = self.cluster_state.selected().unwrap_or(0) as i32;
                let next = (cur + delta).clamp(0, len.saturating_sub(1) as i32);
                self.cluster_state.select(Some(next as usize));
            }
            View::Table(kind) => {
                let len = self.visible_rows(kind).len();
                let cur = self.table_state.selected().unwrap_or(0) as i32;
                let next = (cur + delta).clamp(0, len.saturating_sub(1) as i32);
                self.table_state.select(Some(next as usize));
            }
            _ => {}
        }
    }

    fn select_index(&mut self, idx: usize) {
        match self.current_view() {
            View::Clusters => self.cluster_state.select(Some(idx)),
            View::Table(_) => self.table_state.select(Some(idx)),
            _ => {}
        }
    }

    fn select_last(&mut self) {
        match self.current_view() {
            View::Clusters => {
                let len = self.backend.clusters.len();
                self.cluster_state.select(Some(len.saturating_sub(1)));
            }
            View::Table(kind) => {
                let len = self.visible_rows(kind).len();
                self.table_state.select(Some(len.saturating_sub(1)));
            }
            _ => {}
        }
    }

    fn cycle_namespace(&mut self) {
        let View::Table(kind) = self.current_view() else {
            return;
        };
        if !kind.namespaced() {
            return;
        }
        let names: Vec<String> = self
            .backend
            .namespaces()
            .iter()
            .map(|n| n.name.clone())
            .collect();
        let cur = self.namespace_filter.clone();
        let next = match cur {
            None => names.first().cloned(),
            Some(c) => {
                let pos = names.iter().position(|n| *n == c);
                match pos {
                    Some(i) if i + 1 < names.len() => Some(names[i + 1].clone()),
                    _ => None,
                }
            }
        };
        self.namespace_filter = next;
        self.table_state.select(Some(0));
    }

    fn cycle_kind(&mut self, dir: i32) {
        let View::Table(kind) = self.current_view() else {
            return;
        };
        let all = ResourceKind::ALL;
        let pos = all.iter().position(|k| *k == kind).unwrap_or(0) as i32;
        let len = all.len() as i32;
        let next = ((pos + dir) % len + len) % len;
        self.switch_kind(all[next as usize]);
    }

    fn drill_in(&mut self) {
        match self.current_view() {
            View::Clusters => {
                let idx = self.cluster_state.selected().unwrap_or(0);
                self.backend.select_cluster(idx);
                self.namespace_filter = None;
                self.push_view(View::Table(ResourceKind::Pods));
            }
            View::Table(kind) => {
                let rows = self.visible_rows(kind);
                if let Some(row) = self.table_state.selected().and_then(|i| rows.get(i)) {
                    self.push_view(View::Detail {
                        kind,
                        namespace: row.namespace.clone(),
                        name: row.name.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    fn open_logs(&mut self) {
        let View::Table(ResourceKind::Pods) = self.current_view() else {
            return;
        };
        let rows = self.visible_rows(ResourceKind::Pods);
        if let Some(row) = self.table_state.selected().and_then(|i| rows.get(i)) {
            let namespace = row.namespace.clone().unwrap_or_default();
            let pod = row.name.clone();
            self.log_container = self
                .backend
                .pod_by_name(&namespace, &pod)
                .and_then(|p| p.containers.first().cloned())
                .unwrap_or_else(|| pod.clone());
            self.log_lines.clear();
            self.log_seq = 0;
            self.last_log_tick = Instant::now();
            self.push_view(View::Logs { namespace, pod });
        }
    }

    fn start_create(&mut self) {
        let View::Table(kind) = self.current_view() else {
            return;
        };
        if !kind.creatable() {
            self.status_message = Some(format!("{} can't be created here", kind.title()));
            return;
        }
        self.create_kind = Some(kind);
        self.input.clear();
        self.mode = Mode::CreateName;
    }

    fn start_delete_confirm(&mut self) {
        let View::Table(kind) = self.current_view() else {
            return;
        };
        let rows = self.visible_rows(kind);
        if let Some(row) = self.table_state.selected().and_then(|i| rows.get(i)) {
            self.pending_delete = Some((kind, row.namespace.clone(), row.name.clone()));
            self.mode = Mode::ConfirmDelete;
        }
    }

    fn start_edit(&mut self) {
        let View::Table(kind) = self.current_view() else {
            return;
        };
        if !kind.editable() {
            self.status_message = Some(format!("{} can't be edited here", kind.title()));
            return;
        }
        let rows = self.visible_rows(kind);
        let Some(row) = self.table_state.selected().and_then(|i| rows.get(i)) else {
            return;
        };
        let current = self
            .backend
            .current_edit_value(kind, row.namespace.as_deref(), &row.name)
            .unwrap_or_default();
        self.edit_target = Some((kind, row.namespace.clone(), row.name.clone()));
        self.input = current;
        self.mode = Mode::Edit;
    }

    pub fn create_kind(&self) -> Option<ResourceKind> {
        self.create_kind
    }

    pub fn pending_delete_name(&self) -> Option<&str> {
        self.pending_delete
            .as_ref()
            .map(|(_, _, name)| name.as_str())
    }

    pub fn edit_target_info(&self) -> Option<(ResourceKind, &str)> {
        self.edit_target
            .as_ref()
            .map(|(kind, _, name)| (*kind, name.as_str()))
    }

    pub fn namespace_label(&self) -> &str {
        self.namespace_filter.as_deref().unwrap_or(ALL_NAMESPACES)
    }
}
