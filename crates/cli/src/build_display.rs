//! Presentation of persisted Build facts. No routing, polling, or durable state.
use orchestrate_build::{
    ActionOutcome, BuildEvent, BuildObservation, BuildObserver, BuildPlan, BuildState, Gate, Scope,
    Status, StopKind,
};
use std::{
    collections::HashSet,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use crate::command_display::{command_prefix, shell_quote};

const RULE: &str = "────────────────────────────────────";
const EMPTY: &str = "—";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Plain,
    Terminal,
}

impl Mode {
    // Unsupported explicit values conservatively select plain output. They
    // neither reject a Build nor override its execution semantics.
    fn resolve(override_value: Option<&str>, no_color: Option<&str>, stderr_tty: bool) -> Self {
        match override_value {
            Some("always") => Self::Terminal,
            Some("never") => Self::Plain,
            Some("auto") => {
                if stderr_tty {
                    Self::Terminal
                } else {
                    Self::Plain
                }
            }
            Some(_) => Self::Plain,
            None if no_color.is_some_and(|value| !value.is_empty()) => Self::Plain,
            None => {
                if stderr_tty {
                    Self::Terminal
                } else {
                    Self::Plain
                }
            }
        }
    }

    pub(crate) fn stderr() -> Self {
        let override_value = std::env::var_os("ORCHESTRATE_BUILD_COLOR");
        let no_color = std::env::var_os("NO_COLOR");
        Self::resolve(
            override_value
                .as_ref()
                .map(|value| value.to_str().unwrap_or("invalid")),
            no_color
                .as_ref()
                .map(|value| value.to_str().unwrap_or("nonempty")),
            io::stderr().is_terminal(),
        )
    }
}

#[derive(Clone, Copy)]
enum Tone {
    Current,
    Pass,
    Attention,
    Stop,
    Secondary,
}

pub(crate) struct Display<W> {
    writer: W,
    mode: Mode,
    started: bool,
    frame_lines: usize,
    seen: Vec<(BuildEvent, Option<String>)>,
    reviewed: HashSet<usize>,
    failed: bool,
    root: Option<PathBuf>,
}

impl<W: Write> Display<W> {
    pub(crate) fn new(writer: W, mode: Mode) -> Self {
        Self {
            writer,
            mode,
            started: false,
            frame_lines: 0,
            seen: Vec::new(),
            reviewed: HashSet::new(),
            failed: false,
            root: None,
        }
    }

    pub(crate) fn with_root(mut self, root: Option<PathBuf>) -> Self {
        self.root = root;
        self
    }

    fn paint(&self, tone: Tone, text: &str) -> String {
        if self.mode == Mode::Plain {
            return text.to_owned();
        }
        let code = match tone {
            Tone::Current => "36",
            Tone::Pass => "32",
            Tone::Attention => "33",
            Tone::Stop => "31",
            Tone::Secondary => "2",
        };
        format!("\x1b[{code}m{text}\x1b[0m")
    }

    fn frame(&self, state: &BuildState, plan: Option<&BuildPlan>) -> String {
        let progress = match counts(&state.scope, plan) {
            Some((reviewed, total)) => {
                let filled = ((reviewed as u128 * 20) / total as u128) as usize;
                format!(
                    "[{}{}]  {reviewed} / {total} phases reviewed",
                    "█".repeat(filled),
                    "░".repeat(20 - filled)
                )
            }
            None => "[????????????????????]  Phases reviewed: unavailable".into(),
        };
        let status_tone = match state.status {
            Status::Complete => Tone::Pass,
            Status::Stopped => Tone::Stop,
            _ if state.gate == Gate::Unblock => Tone::Attention,
            _ => Tone::Current,
        };
        let stop = state
            .stop
            .as_ref()
            .map(|stop| stop_kind(&stop.kind))
            .unwrap_or(EMPTY);
        format!(
            "{RULE}\n\n{progress}\n\nStatus:       {}\nGate:         {}\nScope:        {}\nAction:       {}\nCheckpoint:   {}\nStop:         {}\n\n{RULE}\n",
            self.paint(status_tone, status(&state.status)),
            self.paint(
                if state.gate == Gate::Unblock {
                    Tone::Attention
                } else {
                    status_tone
                },
                gate(&state.gate)
            ),
            scope(&state.scope, plan),
            self.paint(Tone::Secondary, &action(state)),
            self.paint(Tone::Secondary, &checkpoint(state)),
            if state.stop.is_some() {
                self.paint(Tone::Stop, stop)
            } else {
                stop.to_owned()
            },
        )
    }

    fn seed_history(&mut self, observation: &BuildObservation<'_>) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some((count, _)) = counts(&observation.state.scope, observation.plan) {
            let plan = observation.plan.unwrap();
            for index in 0..count {
                if self.reviewed.insert(index) {
                    if lines.is_empty() {
                        lines.push(self.paint(
                            Tone::Secondary,
                            "Confirmed prior phase reviews (from saved scope):",
                        ));
                    }
                    lines.push(self.paint(
                        Tone::Pass,
                        &format!(
                            "✓ Phase {} reviewed — {}",
                            index + 1,
                            visible(phase_label(&plan.phases[index]))
                        ),
                    ));
                }
            }
        }
        lines
    }

    fn event_lines(&mut self, observation: &BuildObservation<'_>) -> Vec<String> {
        let state = observation.state;
        let plan = observation.plan;
        let text = match &observation.event {
            BuildEvent::Snapshot => return Vec::new(),
            BuildEvent::Initialized => self.paint(Tone::Current, "→ Build initialized"),
            BuildEvent::Continued => self.paint(
                Tone::Current,
                &format!(
                    "→ Build continued — {} / {} — {}",
                    status(&state.status),
                    gate(&state.gate),
                    scope(&state.scope, plan)
                ),
            ),
            BuildEvent::ActionStarted => {
                let tone = if state.gate == Gate::Unblock {
                    Tone::Attention
                } else {
                    Tone::Current
                };
                format!(
                    "{}\n  Status: {} | Action: {} | Checkpoint: {}",
                    self.paint(
                        tone,
                        &format!(
                            "→ {} started — {}",
                            gate(&state.gate),
                            scope(&state.scope, plan)
                        )
                    ),
                    status(&state.status),
                    self.paint(Tone::Secondary, &action(state)),
                    self.paint(Tone::Secondary, &checkpoint(state))
                )
            }
            BuildEvent::ActionRouted(finished) => {
                let location = scope(&finished.scope, plan);
                let (tone, description) = match (&finished.gate, &finished.outcome) {
                    (Gate::Review, ActionOutcome::Pass) => {
                        if let Scope::Phase { index } = finished.scope {
                            if counts(&finished.scope, plan).is_some()
                                && self.reviewed.insert(index)
                            {
                                (
                                    Tone::Pass,
                                    format!(
                                        "✓ Phase {} reviewed — {}",
                                        index + 1,
                                        visible(phase_label(&plan.unwrap().phases[index]))
                                    ),
                                )
                            } else {
                                (Tone::Pass, format!("✓ REVIEW passed — {location}"))
                            }
                        } else {
                            (Tone::Pass, format!("✓ REVIEW passed — {location}"))
                        }
                    }
                    (_, ActionOutcome::ChangesRequired) => (
                        Tone::Attention,
                        format!("! {} changes required — {location}", gate(&finished.gate)),
                    ),
                    (_, ActionOutcome::Blocked) => (
                        if state.status == Status::Stopped {
                            Tone::Stop
                        } else {
                            Tone::Attention
                        },
                        format!("! {} blocked — {location}", gate(&finished.gate)),
                    ),
                    (_, ActionOutcome::Retry) => (
                        Tone::Attention,
                        format!("→ {} retry confirmed — {location}", gate(&finished.gate)),
                    ),
                    (_, ActionOutcome::Pass) => (
                        Tone::Pass,
                        format!("✓ {} passed — {location}", gate(&finished.gate)),
                    ),
                    (_, ActionOutcome::Complete) => (
                        Tone::Pass,
                        format!("✓ {} complete — {location}", gate(&finished.gate)),
                    ),
                };
                format!(
                    "{}\n  Recorded: {} / {} — {} | Checkpoint: {}",
                    self.paint(tone, &description),
                    status(&state.status),
                    gate(&state.gate),
                    scope(&state.scope, plan),
                    self.paint(Tone::Secondary, &checkpoint(state))
                )
            }
            BuildEvent::Terminal => match state.status {
                Status::Complete => self.paint(Tone::Pass, "✓ Build complete"),
                Status::Stopped => self.paint(
                    Tone::Stop,
                    &terminal_stop(
                        observation.effort_id,
                        state,
                        observation.plan,
                        self.root.as_deref(),
                    ),
                ),
                _ => return Vec::new(),
            },
        };
        vec![text]
    }

    fn render(&mut self, observation: BuildObservation<'_>) -> io::Result<()> {
        let first = !self.started;
        let key = (
            observation.event.clone(),
            if observation.event == BuildEvent::ActionStarted {
                observation.state.current_action_id.clone()
            } else {
                None
            },
        );
        let new_event = !self.seen.contains(&key);
        let mut history = if matches!(
            observation.event,
            BuildEvent::Snapshot | BuildEvent::Initialized
        ) {
            self.seed_history(&observation)
        } else {
            Vec::new()
        };
        if new_event {
            history.extend(self.event_lines(&observation));
        }
        if first {
            writeln!(self.writer, "Orchestrate Build\n")?;
            self.started = true;
        }
        let frame = self.frame(observation.state, observation.plan);
        if self.mode == Mode::Terminal {
            // The cursor is just below our frame. Clear exactly those lines,
            // bottom to top, stopping before the immutable event history.
            for _ in 0..self.frame_lines {
                write!(self.writer, "\x1b[1A\r\x1b[2K")?;
            }
            if !history.is_empty() {
                writeln!(self.writer, "{}\n", history.join("\n\n"))?;
            }
            write!(self.writer, "{frame}")?;
            self.frame_lines = frame.lines().count();
        } else {
            if first {
                writeln!(self.writer, "{frame}")?;
            }
            if !history.is_empty() {
                writeln!(self.writer, "{}\n", history.join("\n\n"))?;
            }
            if !first && new_event && observation.event == BuildEvent::Terminal {
                writeln!(self.writer, "{frame}")?;
            }
        }
        if new_event {
            self.seen.push(key);
        }
        self.writer.flush()
    }
}

impl<W: Write> BuildObserver for Display<W> {
    fn observe(&mut self, observation: BuildObservation<'_>) {
        // Broken output must not change routing, invoke recovery, or stop Build.
        if !self.failed && self.render(observation).is_err() {
            self.failed = true;
        }
    }
}

fn counts(scope: &Scope, plan: Option<&BuildPlan>) -> Option<(usize, usize)> {
    let plan = plan?;
    let total = plan.phases.len();
    if total == 0 || plan.schema_version != orchestrate_build::BUILD_PLAN_VERSION {
        return None;
    }
    match scope {
        Scope::Phase { index } if *index < total => Some((*index, total)),
        Scope::Final => Some((total, total)),
        _ => None,
    }
}

pub(crate) fn status_report(
    effort_id: &str,
    build_dir: &Path,
    state: Option<&BuildState>,
    plan: Option<&BuildPlan>,
    root: Option<&Path>,
) -> String {
    let mut output = format!("Build status — {}\n\n", visible(effort_id));
    let Some(state) = state else {
        output.push_str(&format!(
            "Status: Uninitialized\nBuild directory: {}\n",
            shell_quote(&build_dir.display().to_string())
        ));
        return output;
    };

    let display = Display::new(Vec::new(), Mode::Plain);
    output.push_str(&display.frame(state, plan));
    if let Some(stop) = &state.stop {
        output.push_str(&format!(
            "\nStop detail:\n{}\n",
            visible_multiline(&stop.detail)
        ));
        if !state.feedback.is_empty() {
            output.push_str("\nRecorded feedback:\n");
            for feedback in &state.feedback {
                output.push_str(&format!(
                    "  {} — {}\n",
                    visible(&feedback.path),
                    visible(&feedback.purpose)
                ));
            }
        }
        if let Some(action_id) = state.current_action_id.as_deref()
            && orchestrate_build::state::current_action_dir(build_dir, action_id).is_ok()
            && build_dir
                .join("actions")
                .join(action_id)
                .join("stderr.txt")
                .is_file()
        {
            output.push_str(&format!(
                "\nProvider stderr:\n  actions/{action_id}/stderr.txt\n"
            ));
        }
        output.push('\n');
        output.push_str(&recovery_guidance(&stop.kind, effort_id, root));
    }
    output
}

fn terminal_stop(
    effort_id: &str,
    state: &BuildState,
    plan: Option<&BuildPlan>,
    root: Option<&Path>,
) -> String {
    let kind = state
        .stop
        .as_ref()
        .map(|stop| stop_kind(&stop.kind))
        .unwrap_or("unavailable");
    let mut output = format!("! Build stopped — {kind}\n");
    if let Some(stop) = &state.stop {
        output.push_str(&recovery_guidance(&stop.kind, effort_id, root));
    }
    if let Some((reviewed, total)) = counts(&state.scope, plan) {
        output.push_str(&format!(
            "  Progress: {reviewed} / {total} phases reviewed\n"
        ));
    }
    output
}

fn recovery_guidance(kind: &StopKind, effort_id: &str, root: Option<&Path>) -> String {
    let prefix = command_prefix(root);
    let effort = shell_quote(effort_id);
    let status = format!("{prefix} build status --effort {effort}");
    let launch = format!("{prefix} build --effort {effort}");
    match kind {
        StopKind::ResetRequired => {
            let reset = format!("{prefix} build reset --effort {effort}");
            format!(
                "Next steps:\n  1. Inspect the failed action and its evidence with `{status}`.\n  2. Check `git status --short` and save any work you need before reset.\n  3. Run `{reset}`. Reset restores the recorded checkpoint, discards tracked edits, removes non-ignored untracked files and directories, preserves ignored files, and requeues the same gate.\n  4. Run `{launch}` to continue.\n"
            )
        }
        StopKind::Blocked => format!(
            "Next steps:\n  1. Review the recorded blocker and feedback with `{status}`.\n  2. Address the blocker described in the feedback.\n  3. Run `{launch}` to continue.\n"
        ),
    }
}

fn visible_multiline(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\n' => "\n".to_owned(),
            '\t' => "\t".to_owned(),
            character if character.is_control() => character.escape_default().to_string(),
            character => character.to_string(),
        })
        .collect()
}

fn phase_label(id: &str) -> &str {
    if let Some(rest) = id.strip_prefix("phase_")
        && let Some((digits, name)) = rest.split_once('_')
        && digits.len() >= 2
        && digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return name;
    }
    id
}

fn scope(value: &Scope, plan: Option<&BuildPlan>) -> String {
    match value {
        Scope::Final => "Final".into(),
        Scope::Phase { index } => match counts(value, plan) {
            Some((_, total)) => format!(
                "Phase {} / {total} — {}",
                index + 1,
                visible(phase_label(&plan.unwrap().phases[*index]))
            ),
            None => "Phase unavailable".into(),
        },
    }
}

fn action(state: &BuildState) -> String {
    state
        .current_action_id
        .as_deref()
        .map(visible)
        .unwrap_or_else(|| EMPTY.into())
}
fn checkpoint(state: &BuildState) -> String {
    if state.checkpoint_commit.is_empty() {
        EMPTY.into()
    } else {
        visible(&state.checkpoint_commit.chars().take(12).collect::<String>())
    }
}
fn gate(gate: &Gate) -> &'static str {
    match gate {
        Gate::Work => "WORK",
        Gate::Review => "REVIEW",
        Gate::Audit => "AUDIT",
        Gate::Unblock => "UNBLOCK",
    }
}
fn status(status: &Status) -> &'static str {
    match status {
        Status::Ready => "Ready",
        Status::Running => "Running",
        Status::Stopped => "Stopped",
        Status::Complete => "Complete",
    }
}
fn stop_kind(kind: &StopKind) -> &'static str {
    match kind {
        StopKind::Blocked => "blocked",
        StopKind::ResetRequired => "reset_required",
    }
}

// Render stored control characters visibly, so an identifier cannot inject ANSI
// or new rows into either mode. Ordinary phase labels remain verbatim.
fn visible(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                character.escape_default().to_string()
            } else {
                character.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
