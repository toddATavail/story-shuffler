/*
 * app.rs
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

use eframe::{App, CreationContext, Frame};
use eframe::emath::Align;
use egui::
{
	Button,
	CentralPanel, Checkbox, Context,
	hex_color,
	Layout,
	Response, RichText,
	ScrollArea, SidePanel,
	TextEdit,
	Ui
};
use egui::scroll_area::ScrollAreaOutput;
#[cfg(target_arch = "wasm32")]
use egui::TopBottomPanel;
use petgraph::{algo::all_simple_paths, graph::{DiGraph, NodeIndex}};
use rand::{thread_rng, seq::SliceRandom};
use regex::Regex;
use serde::{Deserialize, Serialize};

////////////////////////////////////////////////////////////////////////////////
//                             Application model.                             //
////////////////////////////////////////////////////////////////////////////////

/// The complete application state.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct StoryShufflerApp
{
	/// The original manuscript, prior to any mutation.
	original_manuscript: String,

	/// Whether the [section&#32;delimiter](Self::delimiter_pattern) should be
	/// construed as a [regular&#32;expression](Regex).
	delimiter_pattern_is_regex: bool,

	/// The section delimiter, as an uncompiled [regular&#32;expression](Regex).
	delimiter_pattern: String,

	/// The error to present if [delimiter_pattern](Self::delimiter_pattern) is
	/// an invalid [regular&#32;expression](Regex).
	delimiter_regex_error: Option<String>,

	/// The sections of the manuscript, in their original lexical order.
	/// Whitespace is trimmed from the ends of each section.
	original_sections: Vec<String>,

	/// The [constraints][Constraints] of each section, in
	/// [section&#32;order](Self::original_sections).
	constraints: Vec<Constraints>,

	/// The lazy [regular&#32;expression](Regex) for validating comma-separated
	/// section numbers.
	#[serde(skip)]
	sections_regex: Option<Regex>,

	/// The shuffled sections, as indices into the
	/// [original&#32;sections](Self::original_sections) of the _most recently
	/// shuffled manuscript_. Note that this _does not_ have to be the current
	/// manuscript, so this should only be used to index the list at the time of
	/// shuffling.
	shuffled_section_indices: Option<Vec<usize>>,

	/// The lazy shuffled sections, as copies of the
	/// [original&#32;sections](Self::original_sections), maintained in lockstep
	/// with [shuffled_section_indices](Self::shuffled_section_indices).
	shuffled_sections: Option<Vec<String>>
}

impl Default for StoryShufflerApp
{
	fn default() -> Self
	{
		Self {
			original_manuscript: Default::default(),
			delimiter_pattern_is_regex: false,
			delimiter_pattern: DEFAULT_DELIMITER_PATTERN.to_string(),
			delimiter_regex_error: None,
			original_sections: vec![],
			constraints: vec![],
			sections_regex: Some(Regex::new(SECTIONS_LIST_PATTERN).unwrap()),
			shuffled_section_indices: None,
			shuffled_sections: None
		}
	}
}

impl StoryShufflerApp
{
	/// Create the application state, loading any previous state that was
	/// persisted by the last run. Use [`cc`](CreationContext) to customize the
	/// look-and-feel of [`egui`] as appropriate. Load any custom fonts.
	pub fn new(cc: &CreationContext<'_>) -> Self
	{
		if let Some(storage) = cc.storage
		{
			// Attempt to load the previous application state, falling back on
			// fresh application state if retrieval fails.
			Self
			{
				sections_regex: Some(
					Regex::new(SECTIONS_LIST_PATTERN).unwrap()
				),
				..eframe::get_value(
					storage,
					eframe::APP_KEY
				).unwrap_or_default()
			}
		}
		else
		{
			// Storage is not available, so create fresh application state.
			Default::default()
		}
	}

	/// Recompute the manuscript's sections. This might be a consequence of:
	/// * Changing the [intent](Self::delimiter_pattern_is_regex) of the
	///   pattern.
	/// * Changing the [pattern](Self::delimiter_pattern).
	/// * Changing the [manuscript](Self::original_manuscript).
	pub(crate) fn update_sections(&mut self)
	{
		if self.delimiter_pattern_is_regex && !self.delimiter_pattern.is_empty()
		{
			match Regex::new(&self.delimiter_pattern)
			{
				Ok(regex) =>
				{
					self.delimiter_regex_error = None;
					self.original_sections =
						regex.split(&self.original_manuscript)
							.map(|section| section.trim().to_string())
							.collect();
					self.constraints = vec![
						Constraints::default();
						self.original_sections.len()
					];
				},
				Err(e) =>
				{
					self.delimiter_regex_error = Some(e.to_string());
					self.original_sections = vec![];
					self.constraints = vec![];
				}
			}
		}
		else
		{
			self.delimiter_regex_error = None;
			self.original_sections =
				self.original_manuscript.split(&self.delimiter_pattern)
					.map(|section| section.trim().to_string())
					.collect();
			self.constraints = vec![
				Constraints::default();
				self.original_sections.len()
			];
		}
	}
}

////////////////////////////////////////////////////////////////////////////////
//                                Constraints.                                //
////////////////////////////////////////////////////////////////////////////////

/// The constraints placed upon a
/// [manuscript](StoryShufflerApp::original_manuscript)
/// [section](StoryShufflerApp::original_sections). Pseudorandom permutation of
/// the section order must honor the constraints of each section.
#[derive(Clone, Serialize, Deserialize)]
struct Constraints
{
	/// Whether the associated
	/// [manuscript](StoryShufflerApp::original_manuscript)
	/// [section](StoryShufflerApp::original_sections) is locked in place.
	fixed: bool,

	/// The sections which must occur _strictly after_ the associated
	/// [section](StoryShufflerApp::original_sections), e.g., for reasons of
	/// narrative causality, denoted by their **one-based** indices.
	before: Vec<usize>,

	/// The workspace for in-process edits of [`before`](Self::before).
	text_buffer: String,

	/// The [text&#32;buffer](Self::text_buffer) is _prima facie_ valid, i.e.,
	/// it satisfies its lexical requirements if not its semantic ones. Defaults
	/// to `true`, because an empty buffer is well-formed (and even semantically
	/// valid).
	text_buffer_is_valid: bool,

	/// The message to present if a paradox is discovered, i.e., because the
	/// ordering constraints lead to a cycle.
	paradox_error: Option<String>
}

impl Default for Constraints
{
	fn default() -> Self
	{
		Self
		{
			fixed: false,
			before: vec![],
			text_buffer: String::new(),
			text_buffer_is_valid: true,
			paradox_error: None
		}
	}
}

/// Create a directed graph that represents the specified
/// [constraints](Constraints), such that each vertex encodes an index into the
/// supplied slice and each edge represents the predecessor being lexically
/// [prior](Constraints::before) to the successor.
fn compute_graph(constraints: &[Constraints]) -> DiGraph<usize, (), usize>
{
	let mut graph = DiGraph::default();
	let count = constraints.len();
	if count == 0
	{
		// There are no constraints, so save some time and ceremony.
		return graph
	}
	// For simplicity, build the nodes up front.
	for (index, _) in constraints.iter().enumerate()
	{
		// Adjust the index to one-based.
		graph.add_node(index + 1);
	}
	// Now create all of the edges.
	for (index, c) in constraints.iter().enumerate()
	{
		if index == 0 && c.fixed
		{
			// Handle a fixed beginning specially.
			for (successor, _) in constraints.iter().enumerate().skip(1)
			{
				graph.update_edge(
					NodeIndex::new(index),
					NodeIndex::new(successor),
					()
				);
			}
		}
		if index == count - 1 && constraints.last().unwrap().fixed
		{
			// Handle a fixed ending specially.
			for (predecessor, _) in constraints
				.iter().enumerate().take(count - 1)
			{
				graph.update_edge(
					NodeIndex::new(predecessor),
					NodeIndex::new(index),
					()
				);
			}
		}
		for successor in &c.before
		{
			// Adjust the target index to zero-based (because it is
			// one-based).
			graph.update_edge(
				NodeIndex::new(index),
				NodeIndex::new(*successor - 1),
				()
			);
		}
	}
	graph
}

/// Find any cycles from the [constraint](Constraints) specified by `index`.
/// If nonempty, the answered [`Vec`] begins and ends with `index`; if empty,
/// then no cycles were found.
fn find_cycle(
	graph: &DiGraph<usize, (), usize>,
	index: NodeIndex<usize>
) -> Vec<Vec<NodeIndex<usize>>>
{
	// Contrary to what the documentation says, this does not reliably return
	// shortest paths, so patch up the answer before answering.
	all_simple_paths(
		graph,
		index,
		index,
		0,
		None
	).collect()
}

////////////////////////////////////////////////////////////////////////////////
//                                 Frame UI.                                  //
////////////////////////////////////////////////////////////////////////////////

impl App for StoryShufflerApp
{
	/// Update the UI and handle any pending user interaction. May be called
	/// many times per second, so handle any slow activity asynchronously.
	fn update(&mut self, ctx: &Context, _frame: &mut Frame)
	{
		#[cfg(target_arch = "wasm32")]
		self.present_banner(ctx);
		self.present_configuration_sidebar(ctx);
		self.present_output_sidebar(ctx);
		// Note that the manuscript panel must be presented last, because the
		// main component is a CentralPanel.
		self.present_manuscript_panel(ctx);
	}

	/// Called by the framework to save state before shutdown.
	fn save(&mut self, storage: &mut dyn eframe::Storage)
	{
		eframe::set_value(storage, eframe::APP_KEY, self);
	}
}

////////////////////////////////////////////////////////////////////////////////
//                                 Banner UI.                                 //
////////////////////////////////////////////////////////////////////////////////

#[cfg(target_arch = "wasm32")]
impl StoryShufflerApp
{
	/// Display a simple banner (in lieu of a title bar on a native build).
	fn present_banner(&mut self, ctx: &Context)
	{
		TopBottomPanel::top("banner").show(ctx, |ui| {
			ui.centered_and_justified(|ui| {
				ui.set_height(50.0);
				ui.heading(
					RichText::new("Story Shuffler")
						.strong()
						.size(24.0)
				);
			});
        });
	}
}

////////////////////////////////////////////////////////////////////////////////
//                         Configuration sidebar UI.                          //
////////////////////////////////////////////////////////////////////////////////

impl StoryShufflerApp
{
	/// Display the [sidebar][SidePanel] and handle any interactions associated
	/// therewith.
	fn present_configuration_sidebar(&mut self, ctx: &Context)
	{
		SidePanel::left("configuration_panel").show(ctx, |ui| {
			heading(ui, "Parsing").on_hover_ui(|ui| {
				ui.horizontal_wrapped(|ui| {
					ui.spacing_mut().item_spacing.x = 0.0;
					ui.label(
						"Here you can specify how your manuscript will be \
						split into sections. Those sections will appear in the "
					);
					ui.label(RichText::new("Constraints").strong());
					ui.label(
						" section below whenever you submit changes to these \
						options "
					);
					ui.label(RichText::new("or").italics());
					ui.label(" to your manuscript.");
				});
			});
			ui.spacing_mut().item_spacing.y = 3.0;
			ui.horizontal(|ui| {
				if ui.add(
					Checkbox::without_text(&mut self.delimiter_pattern_is_regex)
				).clicked()
				{
					// The user toggled the intention for the pattern (between
					// plain and regex), so update the pattern accordingly.
					self.update_sections();
				}
				ui.hyperlink_to(
					"Use regex",
					"https://docs.rs/regex/latest/regex/#syntax"
				);
			}).response.on_hover_text(
				"Treat the section break as a regular expression rather \
				than just plain text. Click the hyperlink for the official \
				syntax reference."
			);
			ui.horizontal(|ui| {
				ui.label("Section delimiter: ");
				if ui.text_edit_singleline(
					&mut self.delimiter_pattern
				).lost_focus()
				{
					// The user changed the pattern, which might mandate a new
					// regex, so update the pattern accordingly.
					self.update_sections();
				}
			}).response.on_hover_text(
				"Set this to the section break pattern. Your manuscript will \
				be broken into sections at occurrences of this pattern, and \
				whitespace will be trimmed from  the beginning and end of each \
				section."
			);
			ui.separator();
			self.present_regex_error(ui);
			self.present_constraints(ui);
			// Retain additional space, to preserve repositioning of the sash.
			ui.allocate_space(ui.available_size());
		});
	}

	/// Display the specified [regular&#32;expression][Regex] compilation error
	/// on the [UI](Ui).
	fn present_regex_error(&self, ui: &mut Ui)
	{
		if let Some(ref error) = self.delimiter_regex_error
		{
			// The pattern is a regex and the regex is busted, so present the
			// problem. Use monospace so that any syntax errors are properly
			// displayed.
			ui.label(
				RichText::new(error.to_string())
					.monospace()
					.color(hex_color!("#aa0000"))
					.strong()
			);
			ui.separator();
		}
	}

	/// Display the [original&#32;sections](Self::original_sections) along with
	/// their [constraints](Self::constraints).
	fn present_constraints(&mut self, ui: &mut Ui)
	{
		if self.original_sections.len() < 2
		{
			// There's no reason to present sections if there's only one
			// section; this might even be confusing for the user.
			return
		}
		heading(ui, "Constraints").on_hover_ui(|ui| {
			ui.horizontal_wrapped(|ui| {
				ui.spacing_mut().item_spacing.x = 0.0;
				ui.label(
					"Here you can specify how movement of your manuscript's \
					sections can be constrained. When you are satisfied with \
					the constraints, you can click "
				);
				ui.label(RichText::new("🎲 Shuffle").strong());
				ui.label(
					" to obtain a permutation of your \
					manuscript that satisfies your constraints. Any ordering \
					paradoxes will be reported beneath their respective \
					constraints."
				);
			});
		});
		ui.spacing_mut().item_spacing.y = 3.0;
		scrollable_sections(
			ui,
			&(0 .. self.original_sections.len()).collect::<Vec<_>>(),
			&mut self.original_sections,
			Some(&mut self.constraints),
			self.sections_regex.as_ref()
		);
	}
}

////////////////////////////////////////////////////////////////////////////////
//                            Manuscript panel UI.                            //
////////////////////////////////////////////////////////////////////////////////

impl StoryShufflerApp
{
	/// Display the [manuscript&#32;panel][CentralPanel] and handle any
	/// interactions associated therewith.
	fn present_manuscript_panel(&mut self, ctx: &Context)
	{
		CentralPanel::default().show(ctx, |ui| {
			ui.spacing_mut().item_spacing.y = 10.0;
			ui.label(
				"Paste your sectioned manuscript into the text area below \
				to split it into sections. It is not recommended to edit \
				the manuscript in place, but you can. None of your data \
				ever leaves your computer."
			);
			ScrollArea::vertical().max_height(550.0).show(ui, |ui| {
				if ui.add(
					TextEdit::multiline(&mut self.original_manuscript)
						.desired_width(f32::INFINITY)
						.desired_rows(30)
				).changed()
				{
					self.update_sections();
				}
			});
			ui.vertical_centered(|ui| {
				let button = ui.add_enabled(
					self.can_shuffle(),
					Button::new(RichText::new("🎲 Shuffle").strong())
				);
				button.clone().on_hover_ui(|ui| {
					ui.horizontal_wrapped(|ui| {
						ui.spacing_mut().item_spacing.x = 0.0;
						ui.label(
							"Produce a randomized reordering of the \
							manuscript's sections that obeys the established \
							constraints. This also clears any resolved error \
							messages in the "
						);
						ui.label(RichText::new("Constraints").strong());
						ui.label(
							" section. If errors remain, then no reordering \
							is performed."
						);
					});
				});
				if button.clicked()
				{
					if let Some(graph) = self.mark_cycles()
					{
						self.shuffle(graph);
					}
				}
			});
			ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
				ui.spacing_mut().item_spacing.y = 3.0;
				egui::warn_if_debug_build(ui);
				ui.hyperlink_to(
					"(Source on GitHub)",
					"https://github.com/toddATavail/story-shuffler"
				);
				ui.separator();
			});
		});
	}

	/// Determine whether the model is correct and can be shuffled.
	fn can_shuffle(&self) -> bool
	{
		self.original_sections.len() > 1
			&& self.constraints.iter().all(|c|
				c.text_buffer_is_valid
			)
	}

	/// Mark any cycles in the specification of the whole system of
	/// [constraints](Self::constraints). If there were no cycles, then answer
	/// the [graph][DiGraph].
	fn mark_cycles(&mut self) -> Option<DiGraph<usize, (), usize>>
	{
		let graph: DiGraph<usize, (), usize> = compute_graph(&self.constraints);
		let mut cycle_count = 0;
		for index in graph.node_indices()
		{
			let cycles = find_cycle(&graph, index);
			if !cycles.is_empty()
			{
				let mut error = String::new();
				for cycle in cycles
				{
					error.push_str("Paradox detected:\n");
					let mut previous = cycle[0];
					for step in cycle.iter().skip(1)
					{
						// Adjust the indices to one-based for our target
						// audience, i.e., writers.
						error.push_str("\t§");
						error.push_str(&(previous.index() + 1).to_string());
						error.push_str(" must come before §");
						error.push_str(&(step.index() + 1).to_string());
						error.push('\n');
						previous = *step;
					}
				}
				self.constraints[index.index()].paradox_error = Some(error);
				cycle_count += 1;
			}
			else
			{
				self.constraints[index.index()].paradox_error = None;
			}
		}
		if cycle_count == 0
		{
			Some(graph)
		}
		else
		{
			None
		}
	}
}

////////////////////////////////////////////////////////////////////////////////
//                             Output sidebar UI.                             //
////////////////////////////////////////////////////////////////////////////////

impl StoryShufflerApp
{
	/// Shuffle the [sections](Self::original_sections) of the
	/// [manuscript](Self::original_manuscript), in accordance with any
	/// [constraints](Self::constraints) established by the user.
	fn shuffle(&mut self, mut graph: DiGraph<usize, (), usize>)
	{
		let mut indices = vec![];
		let mut shuffled = vec![];
		// The algorithm works by peeling off root sets until nothing remains.
		while graph.node_count() != 0
		{
			// Find the roots of the graph, i.e., those vertices that have no
			// ancestors. These are the sections that are not constrained to
			// appear after some other section(s).
			let roots = graph.node_indices()
				.filter(|index|
					graph.neighbors_directed(
						*index,
						petgraph::Direction::Incoming
					).count() == 0
				)
				.collect::<Vec<NodeIndex<usize>>>();
			// Shuffle the roots and pluck the first one.
			let mut shuffled_roots = roots.clone();
			shuffled_roots.shuffle(&mut thread_rng());
			let root = shuffled_roots.first().unwrap();
			let index = *graph.node_weight(*root).unwrap() - 1;
			indices.push(index);
			shuffled.push(self.original_sections[index].clone());
			// Remove the root from the graph. New sections may become roots as
			// a consequence.
			graph.remove_node(*root);
		}
		self.shuffled_section_indices = Some(indices);
		self.shuffled_sections = Some(shuffled);
	}

	/// Display the [sidebar][SidePanel] and handle any interactions associated
	/// therewith.
	fn present_output_sidebar(&mut self, ctx: &Context)
	{
		SidePanel::right("output_panel").show(ctx, |ui| {
			heading(ui, "Reordering").on_hover_ui(|ui| {
				ui.horizontal_wrapped(|ui| {
					ui.spacing_mut().item_spacing.x = 0.0;
					ui.label("Here you ");
					ui.label(
						if self.shuffled_section_indices.is_none() { "will" }
						else { "can" }
					);
					ui.label(
						" see the latest shuffling of your manuscript, \
						having enforced any constraints defined in the "
					);
					ui.label(RichText::new("Constraints").strong());
					ui.label(" section. ");
					ui.label(RichText::new("🎲 Shuffle").strong());
					if self.shuffled_section_indices.is_none()
					{
						ui.label(" to get your first reordering.");
					}
					else
					{
						ui.label(" to get another reordering.");
					}
					ui.label(
						" Configuration changes do not clear this area, only \
						explicit reshuffles."
					);
				});
			});
			ui.spacing_mut().item_spacing.y = 3.0;
			self.present_results(ui);
			// Retain additional space, to preserve repositioning of the sash.
			ui.allocate_space(ui.available_size());
		});
	}

	/// Display the [shuffled&#32;sections](Self::shuffled_sections) along with
	/// controls for manually tweaking their positions.
	fn present_results(&mut self, ui: &mut Ui)
	{
		let delimiter =
			if self.delimiter_pattern_is_regex { "\n\n* * *\n\n".to_string() }
			else { format!("\n\n{}\n\n", &self.delimiter_pattern) };
		if let Some(ref mut shuffled) = self.shuffled_sections.as_mut()
		{
			if shuffled.len() < 2
			{
				// There's no reason to present sections if there's only one
				// section; this might even be confusing for the user.
				return
			}
			let button = ui.add(
				Button::new(
					RichText::new("📋 Copy to clipboard").strong()
				)
			);
			button.clone().on_hover_ui(|ui| {
				ui.horizontal_wrapped(|ui| {
					ui.spacing_mut().item_spacing.x = 0.0;
					ui.label(
						"Assemble the reordered sections into a new \
						manuscript and copy it to the system clipboard. If the \
						section break is not a regular expression, then it \
						separate sections in the new manuscript verbatim. \
						Otherwise, dinkus ("
					);
					ui.code("* * *");
					ui.label(") will separate the sections.");
				});
			});
			if button.clicked()
			{
				let new_manuscript = shuffled.join(&delimiter);
				ui.output_mut(|clipboard| clipboard.copied_text = new_manuscript);
			}
			ui.separator();
			scrollable_sections(
				ui,
				self.shuffled_section_indices.as_ref().unwrap(),
				shuffled,
				None,
				None
			);
		}
	}
}

////////////////////////////////////////////////////////////////////////////////
//                              Custom widgets.                               //
////////////////////////////////////////////////////////////////////////////////

/// Add a common custom heading to the [UI](Ui).
fn heading(ui: &mut Ui, text: impl Into<String>) -> Response
{
	ui.label(RichText::new(text).heading().color(hex_color!("#aaaaaa")))
}

/// Display a [scrollable&#32;area][ScrollArea] containing the specified
/// sections. If [constraints][Constraints] accompany the sections, then also
/// present the constraints and handle any interactions therewith.
fn scrollable_sections(
	ui: &mut Ui,
	indices: &[usize],
	sections: &mut [String],
	mut constraints: Option<&mut [Constraints]>,
	sections_regex: Option<&Regex>
) -> ScrollAreaOutput<()>
{
	ScrollArea::vertical().show(ui, |ui| {
		for (index, section) in sections.iter().enumerate()
		{
			ui.horizontal(|ui| {
				// Writers are not necessarily programmers, so let's present
				// a one-based index.
				let adjusted = indices[index] + 1;
				ui.label(format!("§{}", adjusted));
				if let Some(constraints) = constraints.as_mut()
				{
					let constraints = &mut constraints[index];
					let fixed = &mut constraints.fixed;
					if index == 0 || index == sections.len() - 1
					{
						ui.checkbox(fixed, "Fixed").on_hover_text(
							format!(
								"Check this box if section §{} should be fixed \
								in place at its current position in the \
								manuscript. This constraint is only available \
								for the first and last sections.",
								adjusted
							)
						);
					}
					if !*fixed
					{
						ui.horizontal(|ui| {
							ui.label("Before §");
							if ui.text_edit_singleline(
								&mut constraints.text_buffer
							).changed()
							{
								if let Some(sections_regex) =
									sections_regex.as_ref()
								{
									// Note that we are storing these as one-based
									// indices, not zero-based.
									if sections_regex.is_match(
										&constraints.text_buffer
									)
									{
										constraints.text_buffer_is_valid = true;
										constraints.before =
											constraints.text_buffer
												.split(',')
												.map(|s|
													s.trim().parse::<usize>()
														.unwrap_or_default()
												)
												.filter(|n| *n != 0)
												.collect();
									} else {
										constraints.text_buffer_is_valid =
											false;
										constraints.before = vec![];
									}
								}
							}
						}).response.on_hover_text(
							"This section must come before any sections \
							mentioned in this comma-separated list of section \
							numbers."
						);
					}
				}
			});
			let mut truncated: String = section.chars().take(79).collect();
			truncated.push('…');
			ui.add_enabled(
				false,
				TextEdit::multiline(&mut truncated)
					.desired_rows(2)
			);
			if let Some(constraints) = constraints.as_ref()
			{
				let constraints = &constraints[index];
				if !constraints.text_buffer_is_valid
				{
					ui.label(
						RichText::new("Invalid list of sections.")
							.color(hex_color!("#aa0000"))
							.strong()
					).on_hover_ui(|ui| {
						ui.horizontal_wrapped(|ui| {
							ui.spacing_mut().item_spacing.x = 0.0;
							ui.label(
								"The section list must be given as a comma-\
								separated list of section numbers, like "
							);
							ui.code("1");
							ui.label(" or ");
							ui.code("2,3");
							ui.label(" or ");
							ui.code("1,3,7,10");
							ui.label(
								". You can also leave the list empty if \
								you don't want to constrain the motion of \
								this section during a "
							);
							ui.label(RichText::new("🎲 Shuffle").strong());
							ui.label(".");
						});
					});
				}
				if let Some(error) = constraints.paradox_error.as_ref()
				{
					ui.label(
						RichText::new(error.to_string())
							.color(hex_color!("#aa0000"))
							.strong()
					).on_hover_ui(|ui| {
						ui.horizontal_wrapped(|ui| {
							ui.spacing_mut().item_spacing.x = 0.0;
							ui.label(
								"This is a paradox in your constraints — this \
								constraint claims to come before itself, maybe \
								indirectly. Once you have fixed the paradox, "
							);
							ui.label(RichText::new("🎲 Shuffle").strong());
							ui.label(" again to clear this error.");
						});

					});
				}
			}
			ui.separator();
		}
	})
}

////////////////////////////////////////////////////////////////////////////////
//                                 Constants.                                 //
////////////////////////////////////////////////////////////////////////////////

/// The default section delimiter, which is _not_ a
/// [regular&#32;expression](Regex). Defaults to dinkus, e.g., `* * *`.
const DEFAULT_DELIMITER_PATTERN: &str = r#"* * *"#;

/// The [regular&#32;expression](Regex) for validating comma-separated lists of
/// section numbers.
const SECTIONS_LIST_PATTERN: &str = r#"^(?:\s*\d+\s*(?:,\s*\d+\s*)*)?$"#;
