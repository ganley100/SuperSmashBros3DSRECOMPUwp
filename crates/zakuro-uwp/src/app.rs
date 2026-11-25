//! the UWP app: a CoreApplication view whose Run loop plays the game a frame
//! at a time, with the Xbox controller, XAudio2 and the swap chain.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::core::{implement, Ref, Result, HSTRING};
use windows::ApplicationModel::Activation::IActivatedEventArgs;
use windows::ApplicationModel::Core::{CoreApplication, CoreApplicationView, IFrameworkView, IFrameworkViewSource, IFrameworkView_Impl, IFrameworkViewSource_Impl};
use windows::Foundation::{EventHandler, TypedEventHandler};
use windows::Gaming::Input::{Gamepad, GamepadReading};
use windows::Graphics::Display::DisplayInformation;
use windows::UI::Core::{BackRequestedEventArgs, CoreProcessEventsOption, CoreWindow, CoreWindowEventArgs, SystemNavigationManager, VisibilityChangedEventArgs, WindowSizeChangedEventArgs};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use zakuro_core::{FrameOutcome, System};

use crate::audio::Audio;
use crate::controls::{Buttons, Controls, Pad, Shortcut};
use crate::graphics::{Frame, Graphics};
use crate::layout::Layout;
use crate::region;

/// starts the app, returning when the system closes it.
pub fn run() -> Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    let source: IFrameworkViewSource = Source.into();
    CoreApplication::Run(&source)
}

#[implement(IFrameworkViewSource)]
struct Source;

impl IFrameworkViewSource_Impl for Source_Impl {
    fn CreateView(&self) -> Result<IFrameworkView> {
        Ok(View::default().into())
    }
}

/// what the window's events tell the loop, set from the event handlers.
#[derive(Default)]
struct Signals {
    closed: AtomicBool,
    hidden: AtomicBool,
    resized: AtomicBool,
}

#[implement(IFrameworkView)]
#[derive(Default)]
struct View {
    window: RefCell<Option<CoreWindow>>,
    signals: Arc<Signals>,
}

impl IFrameworkView_Impl for View_Impl {
    fn Initialize(&self, view: Ref<CoreApplicationView>) -> Result<()> {
        if let Some(view) = view.as_ref() {
            view.Activated(&TypedEventHandler::<CoreApplicationView, IActivatedEventArgs>::new(|_, _| {
                CoreWindow::GetForCurrentThread()?.Activate()
            }))?;
        }
        Ok(())
    }

    fn SetWindow(&self, window: Ref<CoreWindow>) -> Result<()> {
        let Some(window) = window.as_ref() else { return Ok(()) };
        let signals = self.signals.clone();
        window.Closed(&TypedEventHandler::<CoreWindow, CoreWindowEventArgs>::new(move |_, _| {
            signals.closed.store(true, Ordering::Relaxed);
            Ok(())
        }))?;
        let signals = self.signals.clone();
        window.VisibilityChanged(&TypedEventHandler::<CoreWindow, VisibilityChangedEventArgs>::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                signals.hidden.store(!args.Visible()?, Ordering::Relaxed);
            }
            Ok(())
        }))?;
        let signals = self.signals.clone();
        window.SizeChanged(&TypedEventHandler::<CoreWindow, WindowSizeChangedEventArgs>::new(move |_, _| {
            signals.resized.store(true, Ordering::Relaxed);
            Ok(())
        }))?;
        // the B button asks to go back, and on an Xbox an app that doesn't
        // take the request is closed: B is the console's A here
        SystemNavigationManager::GetForCurrentView()?.BackRequested(&EventHandler::<BackRequestedEventArgs>::new(|_, args| {
            if let Some(args) = args.as_ref() {
                args.SetHandled(true)?;
            }
            Ok(())
        }))?;
        *self.window.borrow_mut() = Some(window.clone());
        Ok(())
    }

    fn Load(&self, _entry: &HSTRING) -> Result<()> {
        Ok(())
    }

    fn Run(&self) -> Result<()> {
        let window = self.window.borrow().clone().ok_or_else(windows::core::Error::empty)?;
        window.Activate()?;
        let mut player = Player::new(&window)?;
        let dispatcher = window.Dispatcher()?;
        loop {
            if self.signals.hidden.load(Ordering::Relaxed) {
                // nothing runs while the app can't be seen; wait for events
                dispatcher.ProcessEvents(CoreProcessEventsOption::ProcessOneAndAllPending)?;
            } else {
                dispatcher.ProcessEvents(CoreProcessEventsOption::ProcessAllIfPresent)?;
            }
            if self.signals.closed.load(Ordering::Relaxed) || player.quit {
                break;
            }
            if self.signals.hidden.load(Ordering::Relaxed) {
                continue;
            }
            if self.signals.resized.swap(false, Ordering::Relaxed) {
                player.graphics.resize(pixel_size(&window))?;
            }
            player.frame()?;
        }
        Ok(())
    }

    fn Uninitialize(&self) -> Result<()> {
        Ok(())
    }
}

/// the window's size in pixels, which its bounds give in view pixels.
fn pixel_size(window: &CoreWindow) -> (u32, u32) {
    let bounds = window.Bounds().unwrap_or_default();
    let scale = DisplayInformation::GetForCurrentView().and_then(|display| display.RawPixelsPerViewPixel()).unwrap_or(1.0);
    let size = ((bounds.Width as f64 * scale).round() as u32, (bounds.Height as f64 * scale).round() as u32);
    if size.0 == 0 || size.1 == 0 {
        (1920, 1080)
    } else {
        size
    }
}

/// the app's own folder outside D:\DevelopmentFiles, the package's
/// LocalState, for when that one can't be used.
fn local_state() -> Option<PathBuf> {
    let folder = windows::Storage::ApplicationData::Current().ok()?.LocalFolder().ok()?;
    Some(PathBuf::from(folder.Path().ok()?.to_string()))
}

/// the game, or why there is none, and everything around it.
struct Player {
    graphics: Graphics,
    audio: Option<Audio>,
    controls: Controls,
    layout: Layout,
    system: Option<System>,
    /// why the game isn't running, shown in its place.
    message: Option<String>,
    /// a notice shown for a moment, and until when.
    notice: Option<(String, Instant)>,
    show_fps: bool,
    paused: bool,
    quit: bool,
    frames: u32,
    counting_since: Instant,
    fps: f32,
    top: Option<zakuro_core::Screen>,
    bottom: Option<zakuro_core::Screen>,
}

impl Player {
    fn new(window: &CoreWindow) -> Result<Player> {
        let graphics = Graphics::new(window, pixel_size(window))?;
        let region = region::THIS;
        let folders = crate::folders::choose(crate::folders::preferred(region.folder), local_state());
        crate::logfile::init(&folders.data);
        log::info!(
            "Super Smash Bros. 3DS Recompiled {} for Xbox, {} release, files in {}, recompiled code {}",
            env!("CARGO_PKG_VERSION"),
            region.name,
            folders.data.display(),
            if crate::linked::LINKED { "linked in" } else { "not linked in, interpreting" }
        );
        if let Some(note) = &folders.note {
            log::warn!("{note}");
        }
        let audio = Audio::open(zakuro_core::AUDIO_SAMPLE_RATE).inspect_err(|error| log::warn!("no sound, {error}")).ok();

        let mut search = vec![folders.data.clone()];
        search.extend(folders.search.iter().cloned());
        let (system, message) = match crate::scan::find(&region, &search) {
            Ok(found) => {
                log::info!("starting {}", found.game.display());
                match crate::session::start(&region, &folders.data, &found) {
                    Ok(system) => (Some(system), None),
                    Err(error) => {
                        log::error!("{error}");
                        (None, Some(error))
                    }
                }
            }
            Err(missing) => (None, Some(missing.explain(&region))),
        };
        let message = message.map(|message| match &folders.note {
            Some(note) => format!("{message}\n\n{note}"),
            None => message,
        });
        let notice = system.is_some().then(|| {
            let code = if crate::linked::LINKED { "recompiled code" } else { "interpreter, no recompiled code linked in" };
            (format!("{} release, {code}. View + LB: screen layout", region.name), Instant::now() + Duration::from_secs(6))
        });
        Ok(Player {
            graphics,
            audio,
            controls: Controls::new(),
            layout: Layout::default(),
            system,
            message,
            notice,
            show_fps: false,
            paused: false,
            quit: false,
            frames: 0,
            counting_since: Instant::now(),
            fps: 0.0,
            top: None,
            bottom: None,
        })
    }

    /// runs one frame of the game, when there is one, and shows it.
    fn frame(&mut self) -> Result<()> {
        let (input, shortcuts) = self.controls.update(read_gamepads());
        for shortcut in shortcuts {
            match shortcut {
                Shortcut::NextLayout => {
                    self.layout = self.layout.next();
                    self.notify(self.layout.name().to_owned());
                }
                Shortcut::ToggleFps => self.show_fps = !self.show_fps,
                Shortcut::TogglePause => {
                    self.paused = !self.paused;
                    self.notify(if self.paused { "Paused (View + Menu to resume)" } else { "Resumed" }.to_owned());
                }
                Shortcut::ToggleTouch => {
                    let state = if self.controls.touch_mode { "on: right stick moves, RT touches" } else { "off" };
                    self.notify(format!("Touch cursor {state}"));
                }
            }
        }

        let mut stopped = None;
        if let Some(system) = &mut self.system {
            if !self.paused {
                system.set_input(input);
                let outcome = system.run_frame();
                if let Some(audio) = &mut self.audio {
                    audio.play(&system.take_audio());
                } else {
                    system.take_audio();
                }
                self.top = Some(system.read_screen_scaled(zakuro_common::Screen::Top));
                self.bottom = Some(system.read_screen_scaled(zakuro_common::Screen::Bottom));
                if outcome != FrameOutcome::Completed {
                    stopped = Some(if system.fatal_errors.is_empty() { format!("{outcome:?}") } else { system.fatal_errors.join("\n") });
                }
                self.count_frame();
            }
        }
        if let Some(why) = stopped {
            log::error!("the game stopped: {why}");
            self.message = Some(format!("The game stopped.\n\n{why}\n\nThe log is smash-3ds.log in the app's folder."));
            self.system = None;
        }

        let status = self.status();
        let message = self.message.clone();
        let cursor = self.controls.touch_mode.then(|| (self.controls.cursor, input.touch.is_some()));
        self.graphics.draw(&Frame {
            top: self.top.as_ref(),
            bottom: self.bottom.as_ref(),
            layout: self.layout,
            cursor,
            status: status.as_deref(),
            message: message.as_deref(),
            sharp: false,
        })
    }

    fn notify(&mut self, text: String) {
        self.notice = Some((text, Instant::now() + Duration::from_secs(3)));
    }

    fn count_frame(&mut self) {
        self.frames += 1;
        let elapsed = self.counting_since.elapsed();
        if elapsed >= Duration::from_millis(500) {
            self.fps = self.frames as f32 / elapsed.as_secs_f32();
            self.frames = 0;
            self.counting_since = Instant::now();
        }
    }

    /// the line at the top left: a notice while it lasts, else the frame
    /// rate when asked for.
    fn status(&mut self) -> Option<String> {
        if let Some((text, until)) = &self.notice {
            if Instant::now() < *until {
                return Some(text.clone());
            }
            self.notice = None;
        }
        (self.show_fps && self.system.is_some()).then(|| format!("{:.0} FPS", self.fps))
    }
}

/// every connected controller as one.
fn read_gamepads() -> Pad {
    let Ok(pads) = Gamepad::Gamepads() else { return Pad::default() };
    let count = pads.Size().unwrap_or(0);
    (0..count)
        .filter_map(|index| pads.GetAt(index).ok()?.GetCurrentReading().ok())
        .map(pad)
        .fold(Pad::default(), Pad::merge)
}

fn pad(reading: GamepadReading) -> Pad {
    Pad {
        buttons: Buttons(reading.Buttons.0),
        left: [reading.LeftThumbstickX as f32, reading.LeftThumbstickY as f32],
        right: [reading.RightThumbstickX as f32, reading.RightThumbstickY as f32],
        left_trigger: reading.LeftTrigger as f32,
        right_trigger: reading.RightTrigger as f32,
    }
}
