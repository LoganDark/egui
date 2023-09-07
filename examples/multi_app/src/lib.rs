use std::time::Instant;

use eframe::native::run::{EventResult, WinitApp};
use eframe::UserEvent;
use winit::event::{Event, StartCause};
use winit::event_loop::EventLoopWindowTarget;
use winit::window::Window;

pub struct WinitAppRunner<A: WinitApp> {
	app: A,
	next_repaint_time: Option<Instant>,
	last_result: EventResult
}

impl<A: WinitApp> WinitAppRunner<A> {
	pub fn new(native: A) -> Self {
		Self { app: native, next_repaint_time: None, last_result: EventResult::Wait }
	}

	pub fn app(&self) -> &A { &self.app }
	pub fn app_mut(&mut self) -> &mut A { &mut self.app }

	pub fn process_event(&mut self, event: &Event<UserEvent>, target: &EventLoopWindowTarget<UserEvent>) -> Option<Instant> {
		if matches!(self.last_result, EventResult::Exit) { return None; }

		self.last_result = match event {
			Event::LoopDestroyed => EventResult::Exit,

			// Platform-dependent event handlers to workaround a winit bug
			// See: https://github.com/rust-windowing/winit/issues/987
			// See: https://github.com/rust-windowing/winit/issues/1619
			Event::RedrawEventsCleared if cfg!(windows) => {
				self.next_repaint_time = None;
				self.app.run_ui_and_paint()
			}

			Event::RedrawRequested(window_id) if !cfg!(windows) => {
				if Some(*window_id) == self.app.window().map(Window::id) {
					self.next_repaint_time = None;
					self.app.run_ui_and_paint()
				} else {
					EventResult::Wait
				}
			}

			Event::UserEvent(UserEvent::RequestRepaint { when, frame_nr }) => {
				if self.app.frame_nr() == *frame_nr {
					EventResult::RepaintAt(*when)
				} else {
					EventResult::Wait
				}
			}

			Event::NewEvents(StartCause::ResumeTimeReached { .. }) => EventResult::Wait,

			Event::WindowEvent { window_id, .. } if Some(*window_id) != self.app.window().map(Window::id) => EventResult::Wait,

			event => self.app.on_event(target, event).unwrap_or(EventResult::Exit)
		};

		match self.last_result {
			EventResult::Wait => {}

			EventResult::RepaintNow => {
				if cfg!(windows) {
					// Fix flickering on Windows, see https://github.com/emilk/egui/pull/2280
					self.next_repaint_time = None;
					self.app.run_ui_and_paint();
				} else {
					// Fix for https://github.com/emilk/egui/issues/2425
					self.next_repaint_time = Some(Instant::now());
				}
			}

			EventResult::RepaintNext => self.next_repaint_time = Some(Instant::now()),

			EventResult::RepaintAt(at) => self.next_repaint_time = Some(match self.next_repaint_time {
				Some(next_repaint_time) => next_repaint_time.min(at),
				None => at
			}),

			EventResult::Exit => {
				self.app.save_and_destroy();
				self.next_repaint_time = None;
			}
		};

		if self.next_repaint_time.is_some_and(|next_repaint_time| next_repaint_time <= Instant::now()) {
			self.next_repaint_time.take();
			self.app.window().map(Window::request_redraw);
		}

		self.next_repaint_time
	}

	pub fn last_result(&self) -> &EventResult { &self.last_result }
}

impl<A: WinitApp> Drop for WinitAppRunner<A> {
	fn drop(&mut self) {
		if !matches!(self.last_result, EventResult::Exit) {
			self.app.save_and_destroy();
			self.next_repaint_time = None;
		}
	}
}
