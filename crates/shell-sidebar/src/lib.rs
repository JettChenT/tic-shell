use std::{collections::BTreeMap, time::Duration};

use agent::{AgentBridge, AgentControl, AgentEvent, AgentUpdate};
use gpui::{
    AnyElement, App, Bounds, Context, FocusHandle, FontWeight, MouseButton, ObjectFit, Render,
    Size, Subscription, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, div, img, layer_shell::*, point, prelude::*, px,
};
use persistence::AnnotationStore;
use services::{NiriClient, NiriWindow, WorkspaceSnapshot};
use tic_ui::{DEFAULT_FONT_FAMILY, Theme, sizes};
use tokio::sync::mpsc;

pub const APP_ID: &str = "tic-shell-sidebar";
pub const IPC_SOCKET_BASENAME: &str = "tic-shell-sidebar.sock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarCommand {
    Toggle,
    Show,
    Hide,
    ToggleAgent,
    ShowAgent,
    HideAgent,
}

pub struct Sidebar {
    snapshot: WorkspaceSnapshot,
    annotations: AnnotationStore,
    agent_bridge: Option<AgentBridge>,
    agent_events: Vec<AgentEvent>,
    agent_status: String,
    agent_commands: Vec<agent::AgentCommand>,
    editing_workspace_id: Option<u64>,
    annotation_drafts: BTreeMap<u64, String>,
    annotation_focus: FocusHandle,
    annotation_focus_out: Option<Subscription>,
    sidebar_collapsed: bool,
    agent_pane_collapsed: bool,
    prompt: InputBuffer,
}

impl Sidebar {
    pub fn new(
        annotations: AnnotationStore,
        agent_bridge: Option<AgentBridge>,
        agent_updates: Option<mpsc::UnboundedReceiver<AgentUpdate>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = NiriClient::snapshot().unwrap_or_default();
        let mut sidebar = Self {
            snapshot,
            annotations,
            agent_bridge,
            agent_events: Vec::new(),
            agent_status: "starting".to_string(),
            agent_commands: Vec::new(),
            editing_workspace_id: None,
            annotation_drafts: BTreeMap::new(),
            annotation_focus: cx.focus_handle(),
            annotation_focus_out: None,
            sidebar_collapsed: false,
            agent_pane_collapsed: true,
            prompt: InputBuffer::default(),
        };
        sidebar.start_niri_events(cx);
        if let Some(rx) = agent_updates {
            sidebar.start_agent_updates(rx, cx);
        }
        sidebar
    }

    pub fn visible_width(&self) -> f32 {
        if self.sidebar_collapsed {
            sizes::COLLAPSED_WIDTH
        } else if self.agent_pane_collapsed {
            sizes::WORKSPACE_WIDTH
        } else {
            sizes::WORKSPACE_WIDTH + sizes::DIVIDER_WIDTH + sizes::AGENT_WIDTH
        }
    }

    pub fn command(&mut self, command: SidebarCommand, cx: &mut Context<Self>) {
        match command {
            SidebarCommand::Toggle => self.sidebar_collapsed = !self.sidebar_collapsed,
            SidebarCommand::Show => self.sidebar_collapsed = false,
            SidebarCommand::Hide => self.sidebar_collapsed = true,
            SidebarCommand::ToggleAgent => self.agent_pane_collapsed = !self.agent_pane_collapsed,
            SidebarCommand::ShowAgent => {
                self.sidebar_collapsed = false;
                self.agent_pane_collapsed = false;
            }
            SidebarCommand::HideAgent => self.agent_pane_collapsed = true,
        }
        cx.notify();
    }

    fn start_niri_events(&mut self, cx: &mut Context<Self>) {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while !tx.is_closed() {
                if let Err(err) = NiriClient::stream_snapshots(tx.clone()).await {
                    tracing::debug!("niri event stream unavailable: {err:#}");
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        });

        cx.spawn(async move |this, cx| {
            while let Some(snapshot) = rx.recv().await {
                if this
                    .update(cx, |this, cx| {
                        let previous_key = this.agent_workspace_key();
                        this.snapshot = snapshot;
                        if previous_key != this.agent_workspace_key() {
                            this.notify_agent_workspace();
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn start_agent_updates(
        &mut self,
        mut rx: mpsc::UnboundedReceiver<AgentUpdate>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Some(update) = rx.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.apply_agent_update(update);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_agent_update(&mut self, update: AgentUpdate) {
        match update {
            AgentUpdate::Status { status, .. } => self.agent_status = status,
            AgentUpdate::Snapshot { events, .. } => self.agent_events = events,
            AgentUpdate::Workspace {
                title, commands, ..
            } => {
                if !title.is_empty() {
                    self.snapshot.active_workspace_label = title;
                }
                self.agent_commands = commands;
            }
            AgentUpdate::Event(event) => self.agent_events.push(event),
            AgentUpdate::Stderr(line) => self.agent_events.push(AgentEvent {
                id: format!("stderr:{}", self.agent_events.len()),
                kind: "stderr".to_string(),
                title: "codex-agent".to_string(),
                body: line,
                time: String::new(),
            }),
        }
    }

    fn agent_workspace_key(&self) -> String {
        services::niri::agent_workspace_key(self.snapshot.active_workspace_id)
    }

    fn notify_agent_workspace(&self) {
        let Some(bridge) = self.agent_bridge.clone() else {
            return;
        };
        let key = self.agent_workspace_key();
        let title = self.snapshot.active_workspace_label.clone();
        tokio::spawn(async move {
            if let Err(err) = bridge.notify_workspace(&key, &title).await {
                tracing::warn!("failed to notify agent workspace: {err:#}");
            }
        });
    }

    fn send_prompt(&mut self, cx: &mut Context<Self>) {
        let text = self.prompt.take_trimmed();
        if text.is_empty() {
            return;
        }
        let Some(bridge) = self.agent_bridge.clone() else {
            return;
        };
        let key = self.agent_workspace_key();
        let title = self.snapshot.active_workspace_label.clone();
        tokio::spawn(async move {
            if let Err(err) = bridge.prompt(&key, &title, &text).await {
                tracing::warn!("failed to send agent prompt: {err:#}");
            }
        });
        cx.notify();
    }

    fn send_control(&self, control: AgentControl) {
        let Some(bridge) = self.agent_bridge.clone() else {
            return;
        };
        let key = self.agent_workspace_key();
        let title = self.snapshot.active_workspace_label.clone();
        tokio::spawn(async move {
            if let Err(err) = bridge.control(&key, &title, control).await {
                tracing::warn!("failed to send agent control: {err:#}");
            }
        });
    }

    fn effective_workspace_current(&self, workspace: &services::niri::NiriWorkspace) -> bool {
        workspace.focus.is_current()
    }

    fn effective_window_focused(&self, window: &NiriWindow) -> bool {
        window.focused
    }

    fn focus_workspace(&mut self, idx: i64, cx: &mut Context<Self>) {
        let action = cx.background_spawn(async move { NiriClient::focus_workspace(idx) });
        cx.spawn(async move |this, cx| match action.await {
            Ok(()) => {}
            Err(err) => {
                tracing::warn!("failed to focus workspace {idx}: {err:#}");
                let _ = this.update(cx, |_this, cx| {
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn focus_window(&mut self, id: u64, cx: &mut Context<Self>) {
        let action = cx.background_spawn(async move { NiriClient::focus_window(id) });
        cx.spawn(async move |this, cx| match action.await {
            Ok(()) => {}
            Err(err) => {
                tracing::warn!("failed to focus window {id}: {err:#}");
                let _ = this.update(cx, |_this, cx| {
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn windows_for_workspace_owned(&self, workspace_id: u64) -> Vec<NiriWindow> {
        self.snapshot
            .windows
            .iter()
            .filter(|window| window.workspace_id == Some(workspace_id))
            .cloned()
            .collect()
    }

    fn begin_annotation_edit(
        &mut self,
        workspace_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self
            .annotations
            .annotation_for_workspace(workspace_id)
            .to_string();
        self.annotation_drafts.insert(workspace_id, value);
        self.editing_workspace_id = Some(workspace_id);
        window.focus(&self.annotation_focus, cx);
        cx.notify();
    }

    fn commit_annotation_edit(&mut self, workspace_id: u64, cx: &mut Context<Self>) {
        let value = self
            .annotation_drafts
            .remove(&workspace_id)
            .unwrap_or_default();
        if let Err(err) = self.annotations.set_annotation(workspace_id, &value) {
            tracing::warn!("failed to save workspace annotation: {err:#}");
        }
        self.editing_workspace_id = None;
        cx.notify();
    }

    fn cancel_annotation_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(workspace_id) = self.editing_workspace_id.take() {
            self.annotation_drafts.remove(&workspace_id);
            cx.notify();
        }
    }

    fn handle_annotation_key(
        &mut self,
        workspace_id: u64,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "enter" => self.commit_annotation_edit(workspace_id, cx),
            "escape" => self.cancel_annotation_edit(cx),
            "backspace" => {
                if let Some(draft) = self.annotation_drafts.get_mut(&workspace_id) {
                    draft.pop();
                    cx.notify();
                }
            }
            _ => {
                if event.keystroke.modifiers.control || event.keystroke.modifiers.alt {
                    return;
                }
                if let Some(ch) = event.keystroke.key_char.as_deref() {
                    self.annotation_drafts
                        .entry(workspace_id)
                        .or_default()
                        .push_str(ch);
                    cx.notify();
                } else if key.chars().count() == 1 {
                    self.annotation_drafts
                        .entry(workspace_id)
                        .or_default()
                        .push_str(key);
                    cx.notify();
                }
            }
        }
    }

    fn render_button(
        label: impl Into<String>,
        title: &'static str,
        theme: &Theme,
        handler: impl Fn(&mut Sidebar, &mut Context<Sidebar>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = label.into();
        div()
            .id(title)
            .w(px(28.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .bg(theme.bg_muted)
            .hover(|s| s.bg(Theme::default().bg_hover))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| handler(this, cx)),
            )
            .child(label)
            .into_any_element()
    }

    fn render_workspace_pane(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = self
            .snapshot
            .workspaces
            .clone()
            .iter()
            .map(|workspace| {
                let workspace_id = workspace.id;
                let idx = workspace.idx;
                let current = self.effective_workspace_current(workspace);
                let windows = self.windows_for_workspace_owned(workspace_id);
                let annotation_value = self
                    .annotations
                    .annotation_for_workspace(workspace_id)
                    .to_string();
                let editing = self.editing_workspace_id == Some(workspace_id);
                let annotation_text = if editing {
                    self.annotation_drafts
                        .get(&workspace_id)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    annotation_value
                };
                div()
                    .id(format!("workspace-{workspace_id}"))
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .p(px(9.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(if current { theme.accent } else { theme.border })
                    .bg(if current {
                        theme.bg_hover
                    } else {
                        theme.bg_muted
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| this.focus_workspace(idx, cx)),
                    )
                    .child(
                        div()
                            .h(px(25.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(30.0))
                                    .h(px(24.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .bg(if current {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if current { theme.bg } else { theme.text })
                                    .truncate()
                                    .child(workspace.label.clone()),
                            )
                            .child(
                                div()
                                    .id(format!("workspace-annotation-{workspace_id}"))
                                    .flex_1()
                                    .h(px(25.0))
                                    .flex()
                                    .items_center()
                                    .overflow_hidden()
                                    .rounded(px(4.0))
                                    .border_1()
                                    .border_color(if editing {
                                        theme.accent
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .px(px(4.0))
                                    .text_color(if workspace.urgent {
                                        theme.warning
                                    } else if annotation_text.is_empty() && !editing {
                                        theme.text_muted
                                    } else {
                                        theme.text
                                    })
                                    .text_size(px(14.0))
                                    .font_weight(if annotation_text.is_empty() && !editing {
                                        FontWeight::NORMAL
                                    } else {
                                        FontWeight::SEMIBOLD
                                    })
                                    .truncate()
                                    .cursor_pointer()
                                    .when(editing, |this| this.track_focus(&self.annotation_focus))
                                    .on_key_down(cx.listener(
                                        move |this, event: &gpui::KeyDownEvent, _window, cx| {
                                            if this.editing_workspace_id == Some(workspace_id) {
                                                this.handle_annotation_key(workspace_id, event, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            cx.stop_propagation();
                                            this.begin_annotation_edit(workspace_id, _window, cx);
                                        }),
                                    )
                                    .child(if annotation_text.is_empty() && !editing {
                                        "name workspace".to_string()
                                    } else if editing {
                                        format!("{annotation_text}|")
                                    } else {
                                        annotation_text
                                    }),
                            )
                            .when(workspace.urgent, |this| {
                                this.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.warning)
                                        .child("urgent"),
                                )
                            }),
                    )
                    .when(windows.is_empty(), |this| {
                        this.child(
                            div()
                                .h(px(20.0))
                                .flex()
                                .items_center()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child("empty"),
                        )
                    })
                    .children(
                        windows
                            .into_iter()
                            .map(|window| self.render_window_row(&window, theme, cx)),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .w(px(if self.sidebar_collapsed {
                sizes::COLLAPSED_WIDTH
            } else {
                sizes::WORKSPACE_WIDTH
            }))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(if self.sidebar_collapsed { 6.0 } else { 12.0 }))
            .child(
                div()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Self::render_button(
                        if self.sidebar_collapsed { ">" } else { "<" },
                        "toggle-sidebar",
                        theme,
                        |this, cx| this.command(SidebarCommand::Toggle, cx),
                        cx,
                    ))
                    .when(!self.sidebar_collapsed, |this| {
                        this.child(
                            div()
                                .flex_1()
                                .text_size(px(17.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text)
                                .child("Workspaces"),
                        )
                        .child(Self::render_button(
                            "+",
                            "new-workspace",
                            theme,
                            |_this, _cx| {
                                if let Err(err) = NiriClient::focus_workspace(1) {
                                    tracing::warn!("failed to focus workspace: {err:#}");
                                }
                            },
                            cx,
                        ))
                        .child(Self::render_button(
                            "C",
                            "toggle-agent",
                            theme,
                            |this, cx| this.command(SidebarCommand::ToggleAgent, cx),
                            cx,
                        ))
                    }),
            )
            .when(!self.sidebar_collapsed, |this| {
                this.child(
                    div()
                        .id("workspace-list")
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .overflow_y_scroll()
                        .children(rows),
                )
            })
            .into_any_element()
    }

    fn render_window_row(
        &self,
        window: &NiriWindow,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let window_id = window.id;
        let initial = services::niri::app_initial(&window.app_id);
        let icon_path = services::niri::app_icon_path(&window.app_id);
        let has_icon = icon_path.is_some();
        let focused = self.effective_window_focused(window);
        div()
            .id(format!("window-{window_id}"))
            .w_full()
            .h(px(28.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .px(px(7.0))
            .py(px(5.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(if focused { theme.accent } else { theme.border })
            .bg(if focused { theme.accent } else { theme.bg })
            .text_color(if focused { theme.bg } else { theme.text })
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| this.focus_window(window_id, cx)),
            )
            .child(
                div()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .bg(theme.bg_hover)
                    .text_size(px(11.0))
                    .when_some(icon_path, |this, path| {
                        this.child(
                            img(path)
                                .w(px(16.0))
                                .h(px(16.0))
                                .object_fit(ObjectFit::Contain),
                        )
                    })
                    .when(!has_icon, |this| this.child(initial)),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .truncate()
                    .child(window.title.clone()),
            )
            .into_any_element()
    }

    fn render_agent_pane(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let events = self.agent_events.iter().map(|event| {
            let color = match event.kind.as_str() {
                "stderr" | "error" => theme.error,
                "tool" => theme.warning,
                "thinking" => theme.accent,
                _ => theme.text,
            };
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(8.0))
                .rounded(px(7.0))
                .bg(theme.bg_muted)
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_size(px(12.0))
                        .text_color(color)
                        .child(event.title.clone())
                        .child(event.time.clone()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme.text)
                        .child(event.body.clone()),
                )
                .into_any_element()
        });

        div()
            .w(px(sizes::AGENT_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(12.0))
            .child(
                div()
                    .h(px(34.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(17.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Codex"),
                    )
                    .child(Self::render_button(
                        "+",
                        "new-session",
                        theme,
                        |this, _cx| this.send_control(AgentControl::New),
                        cx,
                    ))
                    .child(Self::render_button(
                        "C",
                        "clear-session",
                        theme,
                        |this, _cx| this.send_control(AgentControl::Clear),
                        cx,
                    ))
                    .child(Self::render_button(
                        "x",
                        "cancel-session",
                        theme,
                        |this, _cx| this.send_control(AgentControl::Cancel),
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(
                        if matches!(self.agent_status.as_str(), "error" | "stopped") {
                            theme.error
                        } else {
                            theme.text_muted
                        },
                    )
                    .child(format!(
                        "{} · {}",
                        self.snapshot.active_workspace_label, self.agent_status
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(events),
            )
            .child(
                div()
                    .h(px(58.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.bg_muted)
                    .p(px(8.0))
                    .text_color(if self.prompt.is_empty() {
                        theme.text_muted
                    } else {
                        theme.text
                    })
                    .child(if self.prompt.is_empty() {
                        "Ask Codex for this workspace".to_string()
                    } else {
                        self.prompt.text().to_string()
                    }),
            )
            .into_any_element()
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default();
        let desired = Size::new(px(self.visible_width()), window.viewport_size().height);
        if window.viewport_size() != desired {
            window.resize(desired);
        }
        if self.annotation_focus_out.is_none() {
            self.annotation_focus_out = Some(cx.on_focus_out(
                &self.annotation_focus,
                window,
                |this, _event, _window, cx| {
                    if let Some(workspace_id) = this.editing_workspace_id {
                        this.commit_annotation_edit(workspace_id, cx);
                    }
                },
            ));
        }
        div()
            .id("tic-sidebar")
            .key_context("Sidebar")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(workspace_id) = this.editing_workspace_id {
                        this.commit_annotation_edit(workspace_id, cx);
                    }
                }),
            )
            .on_key_down(
                cx.listener(|this, event: &gpui::KeyDownEvent, _window, cx| {
                    if let Some(workspace_id) = this.editing_workspace_id {
                        this.handle_annotation_key(workspace_id, event, cx);
                        return;
                    }
                    let key = event.keystroke.key.as_str();
                    match key {
                        "enter" => this.send_prompt(cx),
                        "backspace" => this.prompt.backspace(),
                        _ => {
                            if event.keystroke.modifiers.control || event.keystroke.modifiers.alt {
                                return;
                            }
                            if let Some(ch) = event.keystroke.key_char.as_deref() {
                                this.prompt.push_str(ch);
                            } else if key.chars().count() == 1 {
                                this.prompt.push_str(key);
                            }
                        }
                    }
                    cx.notify();
                }),
            )
            .size_full()
            .flex()
            .font_family(DEFAULT_FONT_FAMILY)
            .bg(theme.bg)
            .text_color(theme.text)
            .child(self.render_workspace_pane(&theme, cx))
            .when(
                !self.sidebar_collapsed && !self.agent_pane_collapsed,
                |this| {
                    this.child(div().w(px(sizes::DIVIDER_WIDTH)).h_full().bg(theme.border))
                        .child(self.render_agent_pane(&theme, cx))
                },
            )
    }
}

pub fn window_options(display_id: Option<gpui::DisplayId>, width: f32, cx: &App) -> WindowOptions {
    let display_size = display_id
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display())
        .map(|display| display.bounds().size)
        .unwrap_or_else(|| Size::new(px(1920.0), px(1080.0)));

    WindowOptions {
        display_id,
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: Size::new(px(width), display_size.height),
        })),
        app_id: Some(APP_ID.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "tic-shell-agent-sidebar".to_string(),
            layer: Layer::Top,
            anchor: Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
            exclusive_zone: Some(px(width)),
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub fn open(
    annotations: AnnotationStore,
    agent_bridge: Option<AgentBridge>,
    agent_updates: Option<mpsc::UnboundedReceiver<AgentUpdate>>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Sidebar>> {
    let width = sizes::WORKSPACE_WIDTH;
    cx.open_window(window_options(None, width, cx), move |_, cx| {
        cx.new(|cx| Sidebar::new(annotations, agent_bridge, agent_updates, cx))
    })
    .map_err(|err| anyhow::anyhow!("failed to open sidebar window: {err}"))
}

#[derive(Debug, Default)]
struct InputBuffer {
    text: String,
}

impl InputBuffer {
    fn text(&self) -> &str {
        &self.text
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn push_str(&mut self, value: &str) {
        if !value.chars().any(char::is_control) {
            self.text.push_str(value);
        }
    }

    fn backspace(&mut self) {
        self.text.pop();
    }

    fn take_trimmed(&mut self) -> String {
        let text = self.text.trim().to_string();
        self.text.clear();
        text
    }
}
