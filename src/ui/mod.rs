//! The launcher window: what the worker is doing, or why it failed.
//!
//! This is the window the launcher had before the port, drawn with `iced`. The window is
//! `iced`'s own: a fixed 520x260 frame, the platform's title bar, and `iced`'s light theme. It
//! keeps the same surfaces — a phase heading with its detail, a determinate fill for a byte
//! count, a sweeping segment for a phase that has none, and a failure with retry and exit.
//!
//! The worker never touches the UI. [`LauncherApp::update`] runs on the `iced` runtime and is the
//! only place a [`WorkerEvent`] changes state; the worker's events arrive through an ordinary
//! channel, which `wait_for_event` turns into one `Task` per event.

use std::sync::{Arc, Mutex, mpsc::Receiver};
use std::thread::JoinHandle;
use std::time::Instant;

use iced::widget::{Space, button, column, container, progress_bar, row, text};
use iced::{Color, Element, Fill, Length, Subscription, Task, Theme};

use crate::internal::hanafubuki_process::executor::ExecutorManager;
use crate::workflow::{self, LauncherFailure, LauncherPhase, WorkerEvent};

use self::i18n::{I18n, Message as I18nMessage};

mod i18n;

const TEXT: Color = Color::from_rgb(0.122, 0.169, 0.157);
const MUTED: Color = Color::from_rgb(0.337, 0.365, 0.341);
const ERROR: Color = Color::from_rgb(0.518, 0.188, 0.137);
const LOADING_DURATION_SECONDS: f32 = 1.5;
const LOADING_TRACK_UNITS: u16 = 1_000;
const LOADING_SLIDER_UNITS: u16 = 220;

enum ViewState {
    Working {
        title: String,
        detail: String,
        progress: Option<f32>,
    },
    Failed(String),
}

pub struct LauncherApp {
    state: ViewState,
    i18n: I18n,
    receiver: Arc<Mutex<Receiver<WorkerEvent>>>,
    worker: Option<JoinHandle<()>>,
    executor: ExecutorManager,
    animation_started: Instant,
    loading_position: f32,
}

#[derive(Debug, Clone)]
pub enum Message {
    Worker(WorkerEvent),
    AnimationFrame(Instant),
    Retry,
    Exit,
}

/// Opens the launcher window and runs it until Hanafubuki is started, until the user retries or
/// quits after a failure, or until the window is closed.
///
/// The exit code is `0`: whether the launch itself succeeded is what the log and the window say,
/// and the launcher is not the launched application. `iced`'s own failure to start the window is
/// the one case that reports `1`.
pub fn run(executor: ExecutorManager) -> std::process::ExitCode {
    let icon = iced::window::icon::from_file_data(include_bytes!("../../assets/icon.png"), None)
        .expect("decode launcher window icon");

    let result = iced::application(
        move || LauncherApp::new(executor.clone()),
        LauncherApp::update,
        LauncherApp::view,
    )
    .title("Hanafubuki Launcher")
    .theme(theme)
    .subscription(LauncherApp::subscription)
    .window(iced::window::Settings {
        icon: Some(icon),
        ..Default::default()
    })
    .window_size((520.0, 260.0))
    .resizable(false)
    .centered()
    .run();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log::error!(target: "launcher::ui", "the window could not start: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn theme(_state: &LauncherApp) -> Theme {
    Theme::Light
}

impl LauncherApp {
    fn new(executor: ExecutorManager) -> (Self, Task<Message>) {
        let i18n = I18n::from_system();
        let (receiver, worker) = workflow::spawn(executor.clone());
        let receiver = Arc::new(Mutex::new(receiver));
        let task = wait_for_event(receiver.clone());
        let phase = i18n.phase(LauncherPhase::PreparingWorkspace);
        (
            Self {
                state: ViewState::Working {
                    title: phase.title.into(),
                    detail: phase.detail.into(),
                    progress: None,
                },
                i18n,
                receiver,
                worker: Some(worker),
                executor,
                animation_started: Instant::now(),
                loading_position: 0.0,
            },
            task,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AnimationFrame(now) => {
                self.loading_position = loading_position(self.animation_started, now);
                Task::none()
            }
            Message::Worker(WorkerEvent::Phase(phase)) => {
                let phase = self.i18n.phase(phase);
                self.state = ViewState::Working {
                    title: phase.title.into(),
                    detail: phase.detail.into(),
                    progress: None,
                };
                wait_for_event(self.receiver.clone())
            }
            Message::Worker(WorkerEvent::FileProgress(progress)) => {
                if let ViewState::Working {
                    detail,
                    progress: visible_progress,
                    ..
                } = &mut self.state
                {
                    *detail = self.i18n.t(I18nMessage::FileProgress {
                        operation: progress.operation,
                        completed: progress.completed,
                        total: progress.total,
                        current_path: &progress.current_path,
                    });
                    *visible_progress = progress
                        .total
                        .filter(|total| *total > 0)
                        .map(|total| progress.completed.min(total) as f32 / total as f32);
                }
                wait_for_event(self.receiver.clone())
            }
            Message::Worker(WorkerEvent::Download {
                version,
                completed,
                total,
            }) => {
                self.state = ViewState::Working {
                    title: self
                        .i18n
                        .t(I18nMessage::DownloadingHanafubuki { version: &version }),
                    detail: self
                        .i18n
                        .t(I18nMessage::DownloadProgress { completed, total }),
                    progress: total
                        .filter(|total| *total > 0)
                        .map(|total| completed as f32 / total as f32),
                };
                wait_for_event(self.receiver.clone())
            }
            Message::Worker(WorkerEvent::LauncherDownload {
                version,
                completed,
                total,
            }) => {
                self.state = ViewState::Working {
                    title: self
                        .i18n
                        .t(I18nMessage::UpdatingLauncher { version: &version }),
                    detail: self
                        .i18n
                        .t(I18nMessage::DownloadProgress { completed, total }),
                    progress: total
                        .filter(|total| *total > 0)
                        .map(|total| completed as f32 / total as f32),
                };
                wait_for_event(self.receiver.clone())
            }
            #[cfg(windows)]
            Message::Worker(WorkerEvent::VcRuntimeDownload { completed, total }) => {
                if let ViewState::Working {
                    detail,
                    progress: visible_progress,
                    ..
                } = &mut self.state
                {
                    *detail = self
                        .i18n
                        .t(I18nMessage::DownloadProgress { completed, total });
                    *visible_progress = total
                        .filter(|total| *total > 0)
                        .map(|total| completed.min(total) as f32 / total as f32);
                }
                wait_for_event(self.receiver.clone())
            }
            Message::Worker(WorkerEvent::Scan {
                files,
                directories,
                webuis,
                current_path,
            }) => {
                let phase = self.i18n.phase(LauncherPhase::ScanningWebUis);
                self.state = ViewState::Working {
                    title: phase.title.into(),
                    detail: self.i18n.t(I18nMessage::ScanProgress {
                        files,
                        directories,
                        webuis,
                        current_path: &current_path,
                    }),
                    progress: None,
                };
                wait_for_event(self.receiver.clone())
            }
            Message::Worker(WorkerEvent::Failed(failure)) => {
                self.join_worker();
                self.state = ViewState::Failed(self.i18n.failure(&failure));
                Task::none()
            }
            Message::Worker(WorkerEvent::Launched | WorkerEvent::Restarted) => {
                self.join_worker();
                iced::exit()
            }
            Message::Retry => {
                self.join_worker();
                let (receiver, worker) = workflow::spawn(self.executor.clone());
                self.receiver = Arc::new(Mutex::new(receiver));
                self.worker = Some(worker);
                let phase = self.i18n.phase(LauncherPhase::PreparingWorkspace);
                self.state = ViewState::Working {
                    title: phase.title.into(),
                    detail: self.i18n.t(I18nMessage::RetryingPortableSetup),
                    progress: None,
                };
                wait_for_event(self.receiver.clone())
            }
            Message::Exit => iced::exit(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match &self.state {
            ViewState::Working {
                title,
                detail,
                progress,
            } => {
                let progress: Element<'_, Message> = match progress {
                    Some(progress) => progress_bar(0.0..=1.0, (*progress).clamp(0.0, 1.0)).into(),
                    None => loading_bar(self.loading_position),
                };
                column![
                    text(title).size(25).color(TEXT),
                    text(detail).size(14).color(MUTED),
                    progress,
                ]
                .spacing(18)
                .into()
            }
            ViewState::Failed(error) => column![
                text(self.i18n.t(I18nMessage::PreparationFailed))
                    .size(23)
                    .color(TEXT),
                text(error).size(13).color(ERROR),
                row![
                    button(text(self.i18n.t(I18nMessage::Retry))).on_press(Message::Retry),
                    button(text(self.i18n.t(I18nMessage::Exit))).on_press(Message::Exit),
                ]
                .spacing(10),
            ]
            .spacing(16)
            .into(),
        };

        container(content)
            .padding([28, 30])
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if matches!(&self.state, ViewState::Working { progress: None, .. }) {
            iced::window::frames().map(Message::AnimationFrame)
        } else {
            Subscription::none()
        }
    }
}

fn loading_position(started: Instant, now: Instant) -> f32 {
    (now.saturating_duration_since(started).as_secs_f32() % LOADING_DURATION_SECONDS)
        / LOADING_DURATION_SECONDS
}

fn loading_bar<'a>(position: f32) -> Element<'a, Message> {
    let (left, slider_width, right) = loading_segment(position);
    let slider = container(Space::new())
        .width(loading_width(slider_width))
        .height(Fill)
        .style(|theme| {
            let style = iced::widget::progress_bar::primary(theme);
            iced::widget::container::Style::default()
                .background(style.bar)
                .border(style.border)
        });
    let contents = row![
        Space::new().width(loading_width(left)),
        slider,
        Space::new().width(loading_width(right)),
    ]
    .width(Fill)
    .height(Fill);
    container(contents)
        .width(Fill)
        .height(30)
        .style(|theme| {
            let style = iced::widget::progress_bar::primary(theme);
            iced::widget::container::Style::default()
                .background(style.background)
                .border(style.border)
        })
        .into()
}

fn loading_width(units: u16) -> Length {
    if units == 0 {
        Length::Fixed(0.0)
    } else {
        Length::FillPortion(units)
    }
}

fn loading_segment(position: f32) -> (u16, u16, u16) {
    let position = position.clamp(0.0, 1.0);
    let slider = f32::from(LOADING_SLIDER_UNITS) / f32::from(LOADING_TRACK_UNITS);
    // Both edges travel the track at the same constant speed; the trailing edge
    // is delayed by exactly one slider length. Entering, sliding, and leaving
    // therefore share one velocity, and the phase durations follow from the
    // slider width instead of being cut into fixed time slices.
    let travelled = position * (1.0 + slider);
    let start = (travelled - slider).max(0.0);
    let end = travelled.min(1.0);
    let left = (f32::from(LOADING_TRACK_UNITS) * start).round() as u16;
    let end = (f32::from(LOADING_TRACK_UNITS) * end).round() as u16;
    let slider_width = end.saturating_sub(left);
    let right = LOADING_TRACK_UNITS.saturating_sub(end);
    (left, slider_width, right)
}

fn wait_for_event(receiver: Arc<Mutex<Receiver<WorkerEvent>>>) -> Task<Message> {
    Task::perform(
        async move {
            receiver
                .lock()
                .map_err(|_| LauncherFailure::EventChannelUnavailable)
                .and_then(|receiver| {
                    receiver
                        .recv()
                        .map_err(|_| LauncherFailure::WorkerStoppedUnexpectedly)
                })
                .unwrap_or_else(WorkerEvent::Failed)
        },
        Message::Worker,
    )
}

#[cfg(test)]
mod tests;
