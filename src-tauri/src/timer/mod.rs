pub mod engine;
pub mod sequence;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{AudioCue, AudioManager};
use crate::db::{queries, DbState};
use crate::settings::Settings;
use crate::tray::{self, TrayState};
use crate::websocket::{self, WsState};

use engine::{EngineHandle, TimerCommand, TimerEvent};
use sequence::{RoundType, SequenceState};

// ---------------------------------------------------------------------------
// Snapshot — serialized to JSON for the frontend
// ---------------------------------------------------------------------------

/// Full timer state snapshot. Sent as the payload of Tauri events and
/// returned by the `timer_get_state` IPC command.
#[derive(Debug, Clone, Serialize)]
pub struct TimerSnapshot {
    /// "work" | "short-break" | "long-break"
    pub round_type: String,
    /// Round type that was active before this one. Empty string on the first round of a session.
    pub previous_round_type: String,
    pub elapsed_secs: u32,
    pub total_secs: u32,
    pub is_running: bool,
    /// True if the timer has been started and then paused (elapsed > 0, not running).
    pub is_paused: bool,
    pub work_round_number: u32,
    pub work_rounds_total: u32,
    /// Monotonically-increasing focus round count since last reset. Used as a
    /// session counter when long breaks are disabled.
    pub session_work_count: u32,
    /// Active session tag attached to focus rounds (None = untagged).
    pub active_tag: Option<String>,
}

/// Outcome of applying new settings to the live timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationApplyOutcome {
    /// Configured duration for the current round did not change.
    Unchanged,
    /// Applied immediately to the active (idle, paused, or running) round.
    AppliedLive,
    /// New duration is <= elapsed_secs while active; will apply on the next round.
    DeferredToNextRound,
}

// ---------------------------------------------------------------------------
// Shared mutable state between the controller and the event-listener thread
// ---------------------------------------------------------------------------

struct TimerShared {
    elapsed_secs: u32,
    total_secs: u32,
    is_running: bool,
    active_tag: Option<String>,
    current_session_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// TimerController — public API registered as Tauri state
// ---------------------------------------------------------------------------

pub struct TimerController {
    engine: EngineHandle,
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    db: DbState,
    app: AppHandle,
    tray: Arc<TrayState>,
}

impl TimerController {
    /// Construct and start the background threads.
    /// Call once from `lib.rs` during Tauri `setup`.
    pub fn new(
        app: AppHandle,
        settings: Settings,
        tray: Arc<TrayState>,
        db: DbState,
    ) -> Self {
        let seq = SequenceState::new(settings.long_break_interval);
        let duration = seq.current_duration_secs(&settings);
        let initial_tag = settings.active_tag.clone();

        let (engine, event_rx) = engine::spawn(duration, Duration::from_secs(1));

        let sequence = Arc::new(Mutex::new(seq));
        let settings_arc = Arc::new(Mutex::new(settings));
        let shared = Arc::new(Mutex::new(TimerShared {
            elapsed_secs: 0,
            total_secs: duration,
            is_running: false,
            active_tag: initial_tag,
            current_session_id: None,
        }));

        // Clone handles for the event-listener thread.
        let seq_thread = Arc::clone(&sequence);
        let settings_thread = Arc::clone(&settings_arc);
        let shared_thread = Arc::clone(&shared);
        let engine_thread = engine.clone();
        let tray_thread = Arc::clone(&tray);
        let db_thread = db.clone();
        let app_thread = app.clone();

        std::thread::Builder::new()
            .name("timer-events".to_string())
            .spawn(move || {
                listen_events(
                    app_thread,
                    event_rx,
                    ListenContext {
                        sequence: seq_thread,
                        settings: settings_thread,
                        shared: shared_thread,
                        engine: engine_thread,
                        tray: tray_thread,
                        db: db_thread,
                    },
                );
            })
            .expect("failed to spawn timer event listener");

        Self {
            engine,
            sequence,
            settings: settings_arc,
            shared,
            db,
            app,
            tray,
        }
    }

    // --- Commands ---

    /// Toggle: start a fresh timer if idle, resume if paused, pause if running.
    pub fn toggle(&self) {
        let s = self.shared.lock().unwrap();
        if s.is_running {
            log::info!("[timer] pause");
            self.engine.send(TimerCommand::Pause);
        } else if s.elapsed_secs > 0 {
            log::info!("[timer] resume");
            self.engine.send(TimerCommand::Resume);
        } else {
            log::info!("[timer] start");
            self.engine.send(TimerCommand::Start);
        }
    }

    pub fn reset(&self) {
        log::info!("[timer] reset");
        self.sequence.lock().unwrap().reset();
        self.engine.send(TimerCommand::Reset);
    }

    /// Restart only the current round's timer without touching the sequence.
    pub fn restart_round(&self) {
        log::info!("[timer] restart round");
        self.engine.send(TimerCommand::Reset);
    }

    pub fn skip(&self) {
        log::info!("[timer] skip");
        self.engine.send(TimerCommand::Skip);
    }

    /// Adjust the active round's duration by a signed delta in seconds (e.g. +300 / -300).
    /// Does not modify persisted settings or active profile.
    pub fn adjust_duration(&self, delta_secs: i32) {
        log::info!("[timer] adjust duration delta={delta_secs}s");
        self.engine.send(TimerCommand::AdjustDuration { delta_secs });
    }

    /// Set or clear the active session tag. If a work session is currently in
    /// progress, updates its DB row immediately so the tag is never lost.
    pub fn set_active_tag(&self, tag: Option<String>) -> TimerSnapshot {
        let clean = tag
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        let (session_id, is_work) = {
            let mut sh = self.shared.lock().unwrap();
            sh.active_tag = clean.clone();
            let seq = self.sequence.lock().unwrap();
            (sh.current_session_id, seq.current_round == RoundType::Work)
        };

        if let (Some(id), true) = (session_id, is_work) {
            if let Ok(conn) = self.db.lock() {
                let _ = queries::update_session_tag(&conn, id, clean.as_deref());
            }
        }

        let snap = self.get_snapshot();
        let _ = self.app.emit("timer:adjusted", &snap);
        snap
    }

    pub fn suspend(&self) {
        self.engine.send(TimerCommand::Suspend);
    }

    pub fn wake_resume(&self) {
        self.engine.send(TimerCommand::WakeResume);
    }

    /// Update the duration for the current round when settings change.
    pub fn reconfigure(&self) {
        let duration = {
            let seq = self.sequence.lock().unwrap();
            let settings = self.settings.lock().unwrap();
            seq.current_duration_secs(&settings)
        };
        self.shared.lock().unwrap().total_secs = duration;
        self.engine.send(TimerCommand::Reconfigure { duration_secs: duration });
    }

    // --- Query ---

    pub fn get_snapshot(&self) -> TimerSnapshot {
        build_snapshot(&self.sequence, &self.shared)
    }

    /// Apply new settings values. Updates the in-memory copy and, if the
    /// configured duration for the current round changed, updates the active
    /// round live (when idle or when `new_duration > elapsed_secs`) without
    /// interrupting a running timer.
    pub fn apply_settings(&self, new: Settings) -> DurationApplyOutcome {
        let (old_dur, new_dur) = {
            let mut seq = self.sequence.lock().unwrap();
            seq.work_rounds_total = new.long_break_interval;
            let mut settings_guard = self.settings.lock().unwrap();
            let old_d = seq.current_duration_secs(&settings_guard);
            let new_d = seq.current_duration_secs(&new);
            *settings_guard = new;
            (old_d, new_d)
        };

        if old_dur == new_dur {
            return DurationApplyOutcome::Unchanged;
        }

        let (is_idle, elapsed_secs, is_paused) = {
            let mut sh = self.shared.lock().unwrap();
            let idle = !sh.is_running && sh.elapsed_secs == 0;
            if idle || new_dur > sh.elapsed_secs {
                sh.total_secs = new_dur;
            }
            (idle, sh.elapsed_secs, !sh.is_running && sh.elapsed_secs > 0)
        };

        if is_idle || new_dur > elapsed_secs {
            self.engine.send(TimerCommand::SetTotal {
                duration_secs: new_dur,
            });
            tray::update_title(&self.tray, new_dur.saturating_sub(elapsed_secs), is_paused);
            DurationApplyOutcome::AppliedLive
        } else {
            DurationApplyOutcome::DeferredToNextRound
        }
    }
}

// ---------------------------------------------------------------------------
// Background event listener thread
// ---------------------------------------------------------------------------

struct ListenContext {
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    engine: EngineHandle,
    tray: Arc<TrayState>,
    db: DbState,
}

fn listen_events(
    app: AppHandle,
    event_rx: std::sync::mpsc::Receiver<TimerEvent>,
    ctx: ListenContext,
) {
    let ListenContext {
        sequence,
        settings,
        shared,
        engine,
        tray,
        db,
    } = ctx;
    // Track last tray progress to throttle redraws to ≥ 1% delta.
    let mut last_tray_progress: f32 = -1.0;

    while let Ok(event) = event_rx.recv() {
        match event {
            TimerEvent::Started { total_secs } => {
                log::info!("[timer] started total={total_secs}s");
                {
                    let mut s = shared.lock().unwrap();
                    s.is_running = true;
                    s.total_secs = total_secs;
                }
                let _ = app.emit(
                    "timer:started",
                    serde_json::json!({ "total_secs": total_secs }),
                );
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_started(&ws, total_secs);
                }
                tray::update_menu_items(&tray, true, false);
                tray::update_title(&tray, total_secs, false);
            }

            TimerEvent::Tick {
                elapsed_secs,
                total_secs,
            } => {
                let (needs_insert, active_tag) = {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = elapsed_secs;
                    s.total_secs = total_secs;
                    s.is_running = true;
                    (elapsed_secs == 1 && s.current_session_id.is_none(), s.active_tag.clone())
                };
                let _ = app.emit(
                    "timer:tick",
                    serde_json::json!({ "elapsed_secs": elapsed_secs, "total_secs": total_secs }),
                );

                // --- Session recording: start on first tick of a new round ---
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                if needs_insert {
                    let session_tag = if rt == "work" {
                        active_tag.as_deref()
                    } else {
                        None
                    };
                    if let Ok(conn) = db.lock() {
                        match queries::insert_session(&conn, &rt, total_secs, session_tag) {
                            Ok(id) => {
                                shared.lock().unwrap().current_session_id = Some(id);
                            }
                            Err(e) => log::error!("[timer] failed to record session: {e}"),
                        }
                    }
                }

                // --- Tick sound ---
                if let Some(audio) = app.try_state::<Arc<AudioManager>>() {
                    if audio.tick_enabled_for(&rt) {
                        audio.play_cue(AudioCue::Tick);
                    }
                }

                // Update macOS menu bar countdown every second.
                tray::update_title(&tray, total_secs.saturating_sub(elapsed_secs), false);

                // Update tray arc — throttle to 1% visual change.
                let progress = if total_secs > 0 {
                    elapsed_secs as f32 / total_secs as f32
                } else {
                    0.0
                };
                if (progress - last_tray_progress).abs() >= 0.01 {
                    tray::update_icon(&tray, &rt, false, progress);
                    last_tray_progress = progress;
                }
            }

            TimerEvent::DurationAdjusted {
                elapsed_secs,
                total_secs,
            } => {
                log::info!(
                    "[timer] duration adjusted elapsed={elapsed_secs}s total={total_secs}s"
                );
                let (session_id, is_paused) = {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = elapsed_secs;
                    s.total_secs = total_secs;
                    (s.current_session_id, !s.is_running && elapsed_secs > 0)
                };

                if let Some(id) = session_id {
                    if let Ok(conn) = db.lock() {
                        let _ = queries::update_session_duration(&conn, id, total_secs);
                    }
                }

                let snapshot = build_snapshot(&sequence, &shared);
                let _ = app.emit("timer:adjusted", &snapshot);

                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let progress = if total_secs > 0 {
                    elapsed_secs as f32 / total_secs as f32
                } else {
                    0.0
                };
                tray::update_icon(&tray, &rt, is_paused, progress);
                tray::update_title(&tray, total_secs.saturating_sub(elapsed_secs), is_paused);
                last_tray_progress = progress;
            }

            TimerEvent::Complete {
                skipped: was_skipped,
            } => {
                let completed_round = sequence.lock().unwrap().current_round.as_str().to_string();
                log::info!(
                    "[timer] round complete type={completed_round} skipped={was_skipped}"
                );

                // --- Session recording: mark the completed round ---
                let finished_id = shared.lock().unwrap().current_session_id.take();
                if let Some(session_id) = finished_id {
                    if let Ok(conn) = db.lock() {
                        let _ = queries::complete_session(&conn, session_id, !was_skipped);
                    }
                }

                // Advance sequence.
                let (next_round, next_duration, auto_start_work, auto_start_break) = {
                    let mut seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    let (rt, dur) = seq.advance(&s);
                    (rt, dur, s.auto_start_work, s.auto_start_break)
                };

                // Reset shared state for the new round.
                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = 0;
                    s.total_secs = next_duration;
                    s.is_running = false;
                }

                // Arm the next round's duration without risking a late
                // reconfigure that kicks a freshly-started timer back to Idle.
                engine.send(TimerCommand::Prime {
                    duration_secs: next_duration,
                });

                // Emit round-change with the new snapshot.
                let snapshot = build_snapshot(&sequence, &shared);
                let _ = app.emit("timer:round-change", snapshot);

                // Audio alert for the new round.
                if let Some(audio) = app.try_state::<Arc<AudioManager>>() {
                    let cue = match next_round {
                        RoundType::Work => AudioCue::WorkAlert,
                        RoundType::ShortBreak => AudioCue::ShortBreakAlert,
                        RoundType::LongBreak => AudioCue::LongBreakAlert,
                    };
                    audio.play_cue(cue);
                }

                // Lower-priority-during-breaks.
                let (always_on_top, break_always_on_top) = {
                    let s = settings.lock().unwrap();
                    (s.always_on_top, s.break_always_on_top)
                };
                if always_on_top {
                    if let Some(window) = app.get_webview_window("main") {
                        let is_break = next_round != RoundType::Work;
                        let _ = window.set_always_on_top(!(break_always_on_top && is_break));
                    }
                }

                // Update tray icon & title to reflect new round.
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                tray::update_icon(&tray, &rt, false, 0.0);
                tray::update_title(&tray, next_duration, false);
                last_tray_progress = -1.0;

                // Broadcast round-change to any connected WebSocket clients.
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    let snap = build_snapshot(&sequence, &shared);
                    websocket::broadcast_round_change(&ws, snap);
                }

                // Auto-start if configured.
                let should_auto = match next_round {
                    RoundType::Work => auto_start_work,
                    _ => auto_start_break,
                };
                if should_auto {
                    log::debug!("[timer] auto-starting {}", next_round.as_str());
                    engine.send(TimerCommand::Start);
                } else {
                    tray::update_menu_items(&tray, false, false);
                }
            }

            TimerEvent::Paused { elapsed_secs } => {
                log::info!("[timer] paused elapsed={elapsed_secs}s");
                let total = {
                    let mut s = shared.lock().unwrap();
                    s.is_running = false;
                    s.elapsed_secs = elapsed_secs;
                    s.total_secs
                };
                let _ = app.emit(
                    "timer:paused",
                    serde_json::json!({ "elapsed_secs": elapsed_secs }),
                );
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_paused(&ws, elapsed_secs);
                }

                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let progress = if total > 0 {
                    elapsed_secs as f32 / total as f32
                } else {
                    0.0
                };
                tray::update_icon(&tray, &rt, true, progress);
                tray::update_title(&tray, total.saturating_sub(elapsed_secs), true);
                tray::update_menu_items(&tray, false, true);
            }

            TimerEvent::Resumed { elapsed_secs } => {
                log::info!("[timer] resumed elapsed={elapsed_secs}s");
                let total = {
                    let mut s = shared.lock().unwrap();
                    s.is_running = true;
                    s.elapsed_secs = elapsed_secs;
                    s.total_secs
                };
                let _ = app.emit(
                    "timer:resumed",
                    serde_json::json!({ "elapsed_secs": elapsed_secs }),
                );
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_resumed(&ws, elapsed_secs);
                }

                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let progress = if total > 0 {
                    elapsed_secs as f32 / total as f32
                } else {
                    0.0
                };
                tray::update_icon(&tray, &rt, false, progress);
                tray::update_title(&tray, total.saturating_sub(elapsed_secs), false);
                last_tray_progress = progress;
                tray::update_menu_items(&tray, true, false);
            }

            TimerEvent::Reset => {
                log::debug!("[timer] idle");
                let duration = {
                    let seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.current_duration_secs(&s)
                };

                {
                    let mut s = shared.lock().unwrap();
                    s.current_session_id = None;
                    s.elapsed_secs = 0;
                    s.total_secs = duration;
                    s.is_running = false;
                }
                let snapshot = build_snapshot(&sequence, &shared);
                let _ = app.emit("timer:reset", snapshot);
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_reset(&ws);
                }

                engine.send(TimerCommand::Prime {
                    duration_secs: duration,
                });

                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                tray::update_icon(&tray, &rt, false, 0.0);
                tray::update_title(&tray, duration, false);
                last_tray_progress = -1.0;
                tray::update_menu_items(&tray, false, false);
            }

            TimerEvent::Suspended { elapsed_secs } => {
                log::info!("[timer] suspended by system elapsed={elapsed_secs}s");
                let total = {
                    let mut s = shared.lock().unwrap();
                    s.is_running = false;
                    s.elapsed_secs = elapsed_secs;
                    s.total_secs
                };
                let _ = app.emit(
                    "timer:suspended",
                    serde_json::json!({ "elapsed_secs": elapsed_secs }),
                );

                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let progress = if total > 0 {
                    elapsed_secs as f32 / total as f32
                } else {
                    0.0
                };
                tray::update_icon(&tray, &rt, true, progress);
                tray::update_title(&tray, total.saturating_sub(elapsed_secs), true);
            }
        }
    }
}

fn build_snapshot(
    sequence: &Arc<Mutex<SequenceState>>,
    shared: &Arc<Mutex<TimerShared>>,
) -> TimerSnapshot {
    let seq = sequence.lock().unwrap();
    let sh = shared.lock().unwrap();

    TimerSnapshot {
        round_type: seq.current_round.as_str().to_string(),
        previous_round_type: seq
            .previous_round
            .map(|r| r.as_str().to_string())
            .unwrap_or_default(),
        elapsed_secs: sh.elapsed_secs,
        total_secs: sh.total_secs,
        is_running: sh.is_running,
        is_paused: !sh.is_running && sh.elapsed_secs > 0,
        work_round_number: seq.work_round_number,
        work_rounds_total: seq.work_rounds_total,
        session_work_count: seq.session_work_count,
        active_tag: sh.active_tag.clone(),
    }
}
