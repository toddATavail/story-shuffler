/*
 * main.rs
 * Copyright © 2023, Todd L Smith.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice,
 *    this list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright notice,
 *    this list of conditions and the following disclaimer in the documentation
 *    and/or other materials provided with the distribution.
 *
 * 3. Neither the name of the copyright holder nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS “AS IS”
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#![warn(clippy::all, rust_2018_idioms)]

// Hide console window on Windows in release mode.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Entry point for native execution. Set the environment variable `RUST_LOG` to
/// `debug` to log to standard output.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()>
{
	tracing_subscriber::fmt::init();
	eframe::run_native(
		"Story Shuffler",
		eframe::NativeOptions
		{
			min_window_size: Some(egui::Vec2::new(1000.0, 720.0)),
			..Default::default()
		},
		Box::new(|cc| Box::new(story_shuffler::StoryShufflerApp::new(cc)))
	)
}

/// Entry point for web execution. Hook panic reporting and general logging to
/// the web console. Use the name `app-canvas` to bind `eframe` to the DOM;
/// obviously, there needs to be an eponymous canvas in `index.html`.
#[cfg(target_arch = "wasm32")]
fn main()
{
	console_error_panic_hook::set_once();
	tracing_wasm::set_as_global_default();
	let web_options = eframe::WebOptions::default();
	wasm_bindgen_futures::spawn_local(async {
		eframe::start_web(
			"app-canvas",
			web_options,
			Box::new(|cc| Box::new(story_shuffler::StoryShufflerApp::new(cc)))
		)
			.await
			.expect("failed to start eframe");
	});
}
