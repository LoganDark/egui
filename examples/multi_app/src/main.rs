use std::cell::{OnceCell, Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use eframe::{App, Frame, NativeOptions, Storage, UserEvent};
use eframe::emath::Vec2;
use eframe::native::run::EventResult;
use eframe::native::run::wgpu_integration::WgpuWinitApp;
use eframe::egui::{CentralPanel, Context, Visuals};
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::platform::run_return::EventLoopExtRunReturn;

use multi_app::WinitAppRunner;

enum Operation { Destroy, NewWindow }

#[derive(Default)]
struct DemoApp {
	operation: Option<Operation>,
	edit: String
}

impl App for DemoApp {
	fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
		CentralPanel::default().show(ctx, |ui| {
			if ui.button("Destroy window").clicked() {
				self.operation.replace(Operation::Destroy);
			}

			if ui.button("New window").clicked() {
				self.operation.replace(Operation::NewWindow);
			}

			ui.text_edit_singleline(&mut self.edit);
		});
	}
}

trait AppProxyAccessor {
	type App<'a>: App + 'a where Self: 'a;
	type Borrow<'a>: Deref<Target = Self::App<'a>> where Self: 'a;
	type BorrowMut<'a>: DerefMut<Target = Self::App<'a>> where Self: 'a;

	fn borrow(&self) -> Self::Borrow<'_>;
	fn borrow_mut(&mut self) -> Self::BorrowMut<'_>;
}

struct AppProxy<P: AppProxyAccessor>(pub P);

impl<P: AppProxyAccessor> App for AppProxy<P> {
	fn update(&mut self, ctx: &Context, frame: &mut Frame) { self.0.borrow_mut().update(ctx, frame) }
	fn save(&mut self, storage: &mut dyn Storage) { self.0.borrow_mut().save(storage) }
	fn on_close_event(&mut self) -> bool { self.0.borrow_mut().on_close_event() }
	fn on_exit(&mut self) { self.0.borrow_mut().on_exit() }
	fn auto_save_interval(&self) -> Duration { self.0.borrow().auto_save_interval() }
	fn max_size_points(&self) -> Vec2 { self.0.borrow().max_size_points() }
	fn clear_color(&self, visuals: &Visuals) -> [f32; 4] { self.0.borrow().clear_color(visuals) }
	fn persist_native_window(&self) -> bool { self.0.borrow().persist_native_window() }
	fn persist_egui_memory(&self) -> bool { self.0.borrow().persist_egui_memory() }
	fn warm_up_enabled(&self) -> bool { self.0.borrow().warm_up_enabled() }
	fn post_rendering(&mut self, window_size_px: [u32; 2], frame: &Frame) { self.0.borrow_mut().post_rendering(window_size_px, frame) }
}

fn main() {
	let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
	let proxy = Arc::new(Mutex::new(event_loop.create_proxy()));

	let new = || {
		let options = NativeOptions::default();

		let app = Rc::new(OnceCell::new());
		let app_clone = app.clone();

		let native = WgpuWinitApp::new_proxy(proxy.clone(), "hi", options, Box::new(move |_cc| {
			struct Accessor(Rc<OnceCell<RefCell<DemoApp>>>);

			impl AppProxyAccessor for Accessor {
				type App<'a> = DemoApp;
				type Borrow<'a> = Ref<'a, Self::App<'a>>;
				type BorrowMut<'a> = RefMut<'a, Self::App<'a>>;

				fn borrow(&self) -> Self::Borrow<'_> { self.0.get().unwrap().borrow() }
				fn borrow_mut(&mut self) -> Self::BorrowMut<'_> { self.0.get().unwrap().borrow_mut() }
			}

			app_clone.get_or_init(|| RefCell::new(DemoApp::default()));

			Box::new(AppProxy(Accessor(app_clone.clone())))
		}));

		(app, WinitAppRunner::new(native))
	};

	let mut apps = vec![new()];

	event_loop.run_return(move |event, target, flow| {
		let mut i = 0;
		let mut next_repaint_time: Option<Instant> = None;

		while i < apps.len() {
			let (app, runner) = &mut apps[i];

			if let Some(app_next_repaint_time) = runner.process_event(&event, target) {
				next_repaint_time = Some(match next_repaint_time {
					Some(next_repaint_time) => next_repaint_time.min(app_next_repaint_time),
					None => app_next_repaint_time
				});
			} else if matches!(runner.last_result(), EventResult::Exit) {
				apps.swap_remove(i);
				continue;
			}

			match app.get().and_then(|cell| cell.borrow_mut().operation.take()) {
				None => {},

				Some(Operation::Destroy) => {
					apps.swap_remove(i);
					continue;
				}

				Some(Operation::NewWindow) => {
					let (app, mut runner) = new();
					runner.process_event(&Event::Resumed, target);
					apps.push((app, runner));
				}
			};

			i += 1;
		}

		if apps.is_empty() {
			*flow = ControlFlow::Exit;
		} else {
			*flow = match next_repaint_time {
				Some(instant) => if instant <= Instant::now() {
					ControlFlow::Poll
				} else {
					ControlFlow::WaitUntil(instant)
				},

				None => ControlFlow::Wait
			};
		}
	});
}
