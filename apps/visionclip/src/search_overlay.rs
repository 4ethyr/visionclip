use anyhow::Result;
use visionclip_common::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SearchOverlayState {
    Closed,
    Opening,
    Idle,
    Typing,
    Searching,
    Listening,
    Speaking,
    ResultsReady,
    KeyboardSelecting,
    NoResults,
    Indexing,
    Error,
    PermissionRequired,
}

#[cfg(feature = "gtk-overlay")]
pub fn run_search_overlay(config: &AppConfig) -> Result<()> {
    imp::run_search_overlay(config)
}

#[cfg(not(feature = "gtk-overlay"))]
pub fn run_search_overlay(_config: &AppConfig) -> Result<()> {
    anyhow::bail!("search overlay requires rebuilding visionclip with --features gtk-overlay")
}

#[cfg(feature = "gtk-overlay")]
mod imp {
    use super::SearchOverlayState;
    use anyhow::Result;
    use gtk::cairo;
    use gtk::glib::{self, ControlFlow};
    use gtk::prelude::*;
    use gtk4 as gtk;
    use std::{
        cell::{Cell, RefCell},
        f64::consts::TAU,
        fs,
        io::{Read, Write},
        os::unix::net::UnixStream as StdUnixStream,
        path::{Path, PathBuf},
        rc::Rc,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    use uuid::Uuid;
    use visionclip_common::{
        decode_message_payload, encode_message_payload, AppConfig, JobResult, OpenAction,
        SearchHit, SearchHitSource, SearchMode, SearchOpenRequest, SearchRequest, SearchResponse,
        VisionRequest,
    };

    const OVERLAY_WIDTH: i32 = 760;
    const INPUT_ONLY_WINDOW_HEIGHT: i32 = 118;
    const RESULTS_REVEAL_GAP_PX: i32 = 8;
    const RESULTS_MIN_CONTENT_HEIGHT: i32 = 96;
    const RESULTS_MAX_CONTENT_HEIGHT: i32 = 330;
    const RESULT_ROW_HEIGHT_ESTIMATE: i32 = 86;
    const RESULTS_VERTICAL_PADDING: i32 = 16;

    #[derive(Debug)]
    enum OverlayMessage {
        SearchFinished {
            generation: u64,
            query: String,
            result: std::result::Result<SearchResponse, String>,
        },
        OpenFinished {
            result: std::result::Result<String, String>,
        },
    }

    pub fn run_search_overlay(config: &AppConfig) -> Result<()> {
        let overlay_config = config.ui.search_overlay.clone();
        let socket_path = config.socket_path()?;
        let app = gtk::Application::builder()
            .application_id("io.visionclip.search-overlay")
            .build();

        app.connect_activate(move |app| {
            if let Some(window) = app.active_window() {
                window.present();
                return;
            }

            install_css(&overlay_config);

            let (tx, rx) = mpsc::channel::<OverlayMessage>();
            let state = Rc::new(Cell::new(SearchOverlayState::Opening));
            let generation = Rc::new(Cell::new(0_u64));
            let hits_state = Rc::new(RefCell::new(Vec::<SearchHit>::new()));
            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .title("VisionClip Search")
                .decorated(false)
                .resizable(false)
                .default_width(OVERLAY_WIDTH)
                .default_height(INPUT_ONLY_WINDOW_HEIGHT)
                .build();
            window.add_css_class("search-overlay-window");

            let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
            root.add_css_class("search-overlay-root");

            let input_panel = gtk::Overlay::new();
            input_panel.add_css_class("search-input-panel");
            let input_glass = liquid_glass_layer(&overlay_config, GlassLayerKind::Input);
            input_panel.set_child(Some(&input_glass));

            let input_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            input_content.add_css_class("search-input-content");

            let input_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            input_row.add_css_class("search-input-row");

            let icon = leading_icon();
            input_row.append(&icon);

            let entry = gtk::Entry::builder()
                .placeholder_text("Search files, apps, docs or ask VisionClip...")
                .hexpand(true)
                .build();
            entry.add_css_class("search-entry");
            input_row.append(&entry);

            let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            bar.add_css_class("ai-processing-bar");

            input_content.append(&input_row);
            input_content.append(&bar);
            input_panel.add_overlay(&input_content);

            let results_panel = gtk::Overlay::new();
            results_panel.add_css_class("search-results-panel");
            let results_glass = liquid_glass_layer(&overlay_config, GlassLayerKind::Results);
            results_panel.set_child(Some(&results_glass));

            let results_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            results_content.add_css_class("search-results-content");

            let results = gtk::ListBox::new();
            results.add_css_class("search-results");
            results.set_selection_mode(gtk::SelectionMode::Single);
            results.set_activate_on_single_click(false);

            let results_scroller = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vscrollbar_policy(gtk::PolicyType::Automatic)
                .min_content_height(RESULTS_MIN_CONTENT_HEIGHT)
                .max_content_height(RESULTS_MAX_CONTENT_HEIGHT)
                .propagate_natural_height(true)
                .child(&results)
                .build();
            results_scroller.add_css_class("search-results-scroll");
            results_scroller.set_kinetic_scrolling(true);
            results_scroller.set_overlay_scrolling(true);
            results_scroller.set_overflow(gtk::Overflow::Hidden);
            results_content.set_overflow(gtk::Overflow::Hidden);
            results_panel.set_overflow(gtk::Overflow::Hidden);
            results_content.append(&results_scroller);
            results_panel.add_overlay(&results_content);

            let results_revealer = gtk::Revealer::builder()
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .transition_duration(170)
                .reveal_child(false)
                .child(&results_panel)
                .build();

            root.append(&input_panel);
            root.append(&results_revealer);
            window.set_child(Some(&root));

            install_outside_click_to_close(&root, app);
            install_deactivate_to_close(&window, app);

            let state_for_input = Rc::clone(&state);
            let results_for_input = results.clone();
            let results_revealer_for_input = results_revealer.clone();
            let input_panel_for_input = input_panel.clone();
            let window_for_input = window.clone();
            let socket_path_for_input = socket_path.clone();
            let tx_for_input = tx.clone();
            let generation_for_input = Rc::clone(&generation);
            let hits_for_input = Rc::clone(&hits_state);
            entry.connect_changed(move |entry| {
                let query = entry.text().trim().to_string();
                let has_query = !query.is_empty();
                let next_generation = generation_for_input.get().wrapping_add(1);
                generation_for_input.set(next_generation);
                hits_for_input.borrow_mut().clear();
                state_for_input.set(if has_query {
                    SearchOverlayState::Typing
                } else {
                    SearchOverlayState::Idle
                });

                if has_query {
                    input_panel_for_input.remove_css_class("search-input-panel-open");
                    results_revealer_for_input.set_reveal_child(false);
                    clear_rows(&results_for_input);
                    window_for_input.set_default_size(OVERLAY_WIDTH, INPUT_ONLY_WINDOW_HEIGHT);

                    let socket_path_for_search = socket_path_for_input.clone();
                    let tx_for_search = tx_for_input.clone();
                    let generation_for_search = Rc::clone(&generation_for_input);
                    glib::timeout_add_local(Duration::from_millis(85), move || {
                        if generation_for_search.get() == next_generation {
                            spawn_search(
                                socket_path_for_search.clone(),
                                tx_for_search.clone(),
                                next_generation,
                                query.clone(),
                            );
                        }
                        ControlFlow::Break
                    });
                } else {
                    input_panel_for_input.remove_css_class("search-input-panel-open");
                    results_revealer_for_input.set_reveal_child(false);
                    clear_rows(&results_for_input);
                    window_for_input.set_default_size(OVERLAY_WIDTH, INPUT_ONLY_WINDOW_HEIGHT);
                }
            });

            let app_for_entry_key = app.clone();
            let socket_path_for_entry_key = socket_path.clone();
            let tx_for_entry_key = tx.clone();
            let results_for_entry_key = results.clone();
            let results_adjustment_for_entry_key = results_scroller.vadjustment();
            let hits_for_entry_key = Rc::clone(&hits_state);
            let entry_key = gtk::EventControllerKey::new();
            entry_key.set_propagation_phase(gtk::PropagationPhase::Capture);
            entry_key.connect_key_pressed(move |_, key, _, modifiers| {
                if key == gtk::gdk::Key::Escape {
                    app_for_entry_key.quit();
                    return true.into();
                }
                if key == gtk::gdk::Key::Down {
                    select_relative(&results_for_entry_key, &results_adjustment_for_entry_key, 1);
                    return true.into();
                }
                if key == gtk::gdk::Key::Up {
                    select_relative(
                        &results_for_entry_key,
                        &results_adjustment_for_entry_key,
                        -1,
                    );
                    return true.into();
                }
                if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                    let action = open_action_from_modifiers(modifiers);
                    if let Some(hit) =
                        selected_hit(&results_for_entry_key, &hits_for_entry_key.borrow())
                    {
                        spawn_open(
                            socket_path_for_entry_key.clone(),
                            tx_for_entry_key.clone(),
                            hit.result_id,
                            action,
                        );
                        if matches!(action, OpenAction::Open | OpenAction::Reveal) {
                            app_for_entry_key.quit();
                        }
                    }
                    return true.into();
                }
                false.into()
            });
            entry.add_controller(entry_key);

            let app_for_row = app.clone();
            let socket_path_for_row = socket_path.clone();
            let tx_for_row = tx.clone();
            let hits_for_row = Rc::clone(&hits_state);
            results.connect_row_activated(move |_, row| {
                if let Some(hit) = hit_at_row(row, &hits_for_row.borrow()) {
                    spawn_open(
                        socket_path_for_row.clone(),
                        tx_for_row.clone(),
                        hit.result_id,
                        OpenAction::Open,
                    );
                    app_for_row.quit();
                }
            });

            let app_for_escape = app.clone();
            let key = gtk::EventControllerKey::new();
            key.connect_key_pressed(move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape {
                    app_for_escape.quit();
                    return true.into();
                }
                false.into()
            });
            window.add_controller(key);

            let state_for_messages = Rc::clone(&state);
            let generation_for_messages = Rc::clone(&generation);
            let hits_for_messages = Rc::clone(&hits_state);
            let results_for_messages = results.clone();
            let results_scroller_for_messages = results_scroller.clone();
            let results_adjustment_for_messages = results_scroller.vadjustment();
            let results_revealer_for_messages = results_revealer.clone();
            let input_panel_for_messages = input_panel.clone();
            let window_for_messages = window.clone();
            let entry_for_messages = entry.clone();
            glib::timeout_add_local(Duration::from_millis(16), move || {
                while let Ok(message) = rx.try_recv() {
                    match message {
                        OverlayMessage::SearchFinished {
                            generation,
                            query,
                            result,
                        } => {
                            if generation_for_messages.get() != generation
                                || entry_for_messages.text().trim() != query
                            {
                                continue;
                            }
                            match result {
                                Ok(response) if response.hits.is_empty() => {
                                    state_for_messages.set(SearchOverlayState::NoResults);
                                    hits_for_messages.borrow_mut().clear();
                                    clear_rows(&results_for_messages);
                                    results_revealer_for_messages.set_reveal_child(false);
                                    input_panel_for_messages
                                        .remove_css_class("search-input-panel-open");
                                    window_for_messages
                                        .set_default_size(OVERLAY_WIDTH, INPUT_ONLY_WINDOW_HEIGHT);
                                }
                                Ok(response) => {
                                    state_for_messages.set(SearchOverlayState::ResultsReady);
                                    *hits_for_messages.borrow_mut() = response.hits.clone();
                                    results_scroller_for_messages.set_size_request(
                                        -1,
                                        results_content_height_for_count(response.hits.len()),
                                    );
                                    render_hits(&results_for_messages, &response.hits);
                                    if let Some(row) = results_for_messages.row_at_index(0) {
                                        results_for_messages.select_row(Some(&row));
                                    }
                                    results_adjustment_for_messages.set_value(0.0);
                                    input_panel_for_messages
                                        .add_css_class("search-input-panel-open");
                                    results_revealer_for_messages.set_reveal_child(true);
                                    window_for_messages.set_default_size(
                                        OVERLAY_WIDTH,
                                        overlay_window_height_for_results(response.hits.len()),
                                    );
                                }
                                Err(message) => {
                                    state_for_messages.set(SearchOverlayState::Error);
                                    hits_for_messages.borrow_mut().clear();
                                    results_scroller_for_messages
                                        .set_size_request(-1, RESULTS_MIN_CONTENT_HEIGHT);
                                    replace_hint_row(&results_for_messages, &message);
                                    input_panel_for_messages
                                        .add_css_class("search-input-panel-open");
                                    results_revealer_for_messages.set_reveal_child(true);
                                    window_for_messages.set_default_size(
                                        OVERLAY_WIDTH,
                                        overlay_window_height_for_results(1),
                                    );
                                }
                            }
                        }
                        OverlayMessage::OpenFinished { result } => {
                            if let Err(message) = result {
                                state_for_messages.set(SearchOverlayState::Error);
                                hits_for_messages.borrow_mut().clear();
                                results_scroller_for_messages
                                    .set_size_request(-1, RESULTS_MIN_CONTENT_HEIGHT);
                                replace_hint_row(&results_for_messages, &message);
                                input_panel_for_messages.add_css_class("search-input-panel-open");
                                results_revealer_for_messages.set_reveal_child(true);
                                window_for_messages.set_default_size(
                                    OVERLAY_WIDTH,
                                    overlay_window_height_for_results(1),
                                );
                            }
                        }
                    }
                }
                ControlFlow::Continue
            });

            window.present();
            entry.grab_focus();

            let input_glass_for_animation = input_glass.clone();
            let results_glass_for_animation = results_glass.clone();
            glib::timeout_add_local(Duration::from_millis(33), move || {
                input_glass_for_animation.queue_draw();
                results_glass_for_animation.queue_draw();
                ControlFlow::Continue
            });

            let state_for_timer = Rc::clone(&state);
            glib::timeout_add_local(Duration::from_millis(140), move || {
                if state_for_timer.get() == SearchOverlayState::Opening {
                    state_for_timer.set(SearchOverlayState::Idle);
                }
                ControlFlow::Continue
            });
        });

        let args = ["visionclip-search-overlay"];
        let _ = app.run_with_args(&args);
        Ok(())
    }

    fn spawn_search(
        socket_path: PathBuf,
        tx: mpsc::Sender<OverlayMessage>,
        generation: u64,
        query: String,
    ) {
        thread::spawn(move || {
            let request = SearchRequest {
                request_id: Uuid::new_v4().to_string(),
                query: query.clone(),
                mode: SearchMode::Auto,
                root_hint: None,
                limit: 8,
                include_snippets: true,
                include_ocr: false,
                include_semantic: false,
            };
            let result = match send_sync_request(&socket_path, &VisionRequest::Search(request)) {
                Ok(JobResult::Search(response)) => Ok(response),
                Ok(JobResult::Error { code, message, .. }) => {
                    Err(format!("Search error {code}: {message}"))
                }
                Ok(_) => Err("Daemon returned an unexpected search response.".to_string()),
                Err(message) => Err(message),
            };
            let _ = tx.send(OverlayMessage::SearchFinished {
                generation,
                query,
                result,
            });
        });
    }

    fn spawn_open(
        socket_path: PathBuf,
        tx: mpsc::Sender<OverlayMessage>,
        result_id: String,
        action: OpenAction,
    ) {
        thread::spawn(move || {
            let request = SearchOpenRequest {
                request_id: Uuid::new_v4().to_string(),
                result_id,
                action,
            };
            let result = match send_sync_request(&socket_path, &VisionRequest::SearchOpen(request))
            {
                Ok(JobResult::ActionStatus { message, .. }) => Ok(message),
                Ok(JobResult::Error { code, message, .. }) => {
                    Err(format!("Open error {code}: {message}"))
                }
                Ok(_) => Err("Daemon returned an unexpected open response.".to_string()),
                Err(message) => Err(message),
            };
            let _ = tx.send(OverlayMessage::OpenFinished { result });
        });
    }

    fn send_sync_request(
        socket_path: &Path,
        request: &VisionRequest,
    ) -> std::result::Result<JobResult, String> {
        let mut stream = StdUnixStream::connect(socket_path)
            .map_err(|error| format!("Failed to connect to VisionClip daemon: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|error| format!("Failed to configure daemon socket: {error}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|error| format!("Failed to configure daemon socket: {error}"))?;

        let payload = encode_message_payload(request)
            .map_err(|error| format!("Failed to encode daemon request: {error}"))?;
        let length = (payload.len() as u32).to_be_bytes();
        stream
            .write_all(&length)
            .and_then(|_| stream.write_all(&payload))
            .and_then(|_| stream.flush())
            .map_err(|error| format!("Failed to send daemon request: {error}"))?;

        let mut length = [0_u8; 4];
        stream
            .read_exact(&mut length)
            .map_err(|error| format!("Failed to read daemon response: {error}"))?;
        let payload_len = u32::from_be_bytes(length) as usize;
        let mut payload = vec![0_u8; payload_len];
        stream
            .read_exact(&mut payload)
            .map_err(|error| format!("Failed to read daemon response payload: {error}"))?;
        decode_message_payload::<JobResult>(&payload)
            .map_err(|error| format!("Failed to decode daemon response: {error}"))
    }

    fn leading_icon() -> gtk::CenterBox {
        let frame = gtk::CenterBox::new();
        frame.add_css_class("search-leading-icon-frame");
        frame.set_size_request(52, 52);
        frame.set_halign(gtk::Align::Center);
        frame.set_valign(gtk::Align::Center);

        let label = gtk::Label::new(Some("AI"));
        label.add_css_class("search-leading-icon");
        label.set_halign(gtk::Align::Center);
        label.set_valign(gtk::Align::Center);
        label.set_xalign(0.5);
        label.set_yalign(0.5);
        frame.set_center_widget(Some(&label));
        frame
    }

    fn install_deactivate_to_close(window: &gtk::ApplicationWindow, app: &gtk::Application) {
        let close_armed = Rc::new(Cell::new(false));
        let close_armed_for_timer = Rc::clone(&close_armed);
        glib::timeout_add_local_once(Duration::from_millis(450), move || {
            close_armed_for_timer.set(true);
        });

        let app_for_deactivate = app.clone();
        window.connect_is_active_notify(move |window| {
            if close_armed.get() && !window.is_active() {
                app_for_deactivate.quit();
            }
        });
    }

    fn install_outside_click_to_close(root: &gtk::Box, app: &gtk::Application) {
        let app_for_click = app.clone();
        let root_for_click = root.clone();
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_pressed(move |_, _, x, y| {
            let width = f64::from(root_for_click.allocated_width());
            let height = f64::from(root_for_click.allocated_height());
            if is_outside_overlay_content(width, height, 20.0, x, y) {
                app_for_click.quit();
            }
        });
        root.add_controller(click);
    }

    fn is_outside_overlay_content(width: f64, height: f64, margin: f64, x: f64, y: f64) -> bool {
        if width <= margin * 2.0 || height <= margin * 2.0 {
            return false;
        }
        x < margin || y < margin || x > width - margin || y > height - margin
    }

    fn render_hits(results: &gtk::ListBox, hits: &[SearchHit]) {
        clear_rows(results);
        for hit in hits {
            let row = gtk::ListBoxRow::new();
            row.add_css_class("search-result-row");

            let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            content.add_css_class("search-result-content");
            content.set_valign(gtk::Align::Center);

            let icon = result_icon(hit);
            content.append(&icon);

            let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
            text.set_hexpand(true);
            text.set_valign(gtk::Align::Center);

            let title = gtk::Label::new(Some(&hit.title));
            title.add_css_class("search-result-title");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_single_line_mode(true);
            text.append(&title);

            let subtitle_text = hit.snippet.as_deref().unwrap_or(&hit.path);
            let subtitle = gtk::Label::new(Some(subtitle_text));
            subtitle.add_css_class("search-result-subtitle");
            subtitle.set_xalign(0.0);
            subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
            subtitle.set_single_line_mode(true);
            text.append(&subtitle);

            content.append(&text);

            let chip = gtk::Label::new(Some(source_chip(hit)));
            chip.add_css_class("search-result-chip");
            chip.set_size_request(70, -1);
            chip.set_halign(gtk::Align::End);
            chip.set_valign(gtk::Align::Center);
            chip.set_xalign(0.5);
            content.append(&chip);

            row.set_child(Some(&content));
            results.append(&row);
        }
    }

    fn results_content_height_for_count(count: usize) -> i32 {
        let natural = RESULTS_VERTICAL_PADDING + count as i32 * RESULT_ROW_HEIGHT_ESTIMATE;
        natural.clamp(RESULTS_MIN_CONTENT_HEIGHT, RESULTS_MAX_CONTENT_HEIGHT)
    }

    fn overlay_window_height_for_results(count: usize) -> i32 {
        INPUT_ONLY_WINDOW_HEIGHT + RESULTS_REVEAL_GAP_PX + results_content_height_for_count(count)
    }

    fn select_relative(results: &gtk::ListBox, adjustment: &gtk::Adjustment, delta: i32) {
        let Some(first_row) = results.row_at_index(0) else {
            return;
        };
        let current = results.selected_row().map(|row| row.index()).unwrap_or(0);
        let mut next = current + delta;
        if next < 0 {
            next = 0;
        }
        let Some(row) = results.row_at_index(next) else {
            results.select_row(Some(&first_row));
            scroll_row_into_view(&first_row, adjustment);
            return;
        };
        results.select_row(Some(&row));
        scroll_row_into_view(&row, adjustment);
    }

    fn scroll_row_into_view(row: &gtk::ListBoxRow, adjustment: &gtk::Adjustment) {
        let row_top = f64::from(row.allocation().y());
        let row_bottom = row_top + f64::from(row.allocated_height());
        let viewport_top = adjustment.value();
        let viewport_bottom = viewport_top + adjustment.page_size();

        if row_top < viewport_top {
            adjustment.set_value(row_top);
        } else if row_bottom > viewport_bottom {
            adjustment.set_value(row_bottom - adjustment.page_size());
        }
    }

    fn selected_hit(results: &gtk::ListBox, hits: &[SearchHit]) -> Option<SearchHit> {
        let index = results.selected_row().map(|row| row.index()).unwrap_or(0);
        hit_at_index(index, hits)
    }

    fn hit_at_row(row: &gtk::ListBoxRow, hits: &[SearchHit]) -> Option<SearchHit> {
        hit_at_index(row.index(), hits)
    }

    fn hit_at_index(index: i32, hits: &[SearchHit]) -> Option<SearchHit> {
        if index < 0 {
            return None;
        }
        hits.get(index as usize)
            .cloned()
            .or_else(|| hits.first().cloned())
    }

    fn open_action_from_modifiers(modifiers: gtk::gdk::ModifierType) -> OpenAction {
        if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            OpenAction::Reveal
        } else if modifiers.contains(gtk::gdk::ModifierType::ALT_MASK) {
            OpenAction::AskAbout
        } else if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            OpenAction::Summarize
        } else {
            OpenAction::Open
        }
    }

    fn source_chip(hit: &SearchHit) -> &'static str {
        if hit.kind == "app" {
            return "APP";
        }
        if file_extension(&hit.path).as_deref() == Some("pdf") {
            return "PDF";
        }
        match hit.source {
            SearchHitSource::Path => "PATH",
            SearchHitSource::Content => "CONTENT",
            SearchHitSource::Ocr => "OCR",
            SearchHitSource::Semantic => "SEMANTIC",
            SearchHitSource::Recent => "RECENT",
            SearchHitSource::App => "APP",
            SearchHitSource::Document => "DOCUMENT",
            SearchHitSource::Code => "CODE",
            SearchHitSource::FileName => "EXACT",
        }
    }

    fn result_icon(hit: &SearchHit) -> gtk::CenterBox {
        let frame = gtk::CenterBox::new();
        frame.add_css_class("search-result-icon-frame");
        frame.set_size_request(52, 52);
        frame.set_halign(gtk::Align::Center);
        frame.set_valign(gtk::Align::Center);
        let image = if hit.kind == "app" {
            desktop_icon_name(&hit.path)
                .map(|icon| {
                    let icon_path = PathBuf::from(&icon);
                    if icon_path.is_absolute() && icon_path.exists() {
                        gtk::Image::from_file(icon_path)
                    } else {
                        gtk::Image::from_icon_name(&icon)
                    }
                })
                .unwrap_or_else(|| gtk::Image::from_icon_name("application-x-executable"))
        } else {
            gtk::Image::from_icon_name(file_icon_name(hit))
        };
        image.set_pixel_size(24);
        image.add_css_class("search-result-icon");
        image.set_halign(gtk::Align::Center);
        image.set_valign(gtk::Align::Center);
        frame.set_center_widget(Some(&image));
        frame
    }

    fn file_icon_name(hit: &SearchHit) -> &'static str {
        match file_extension(&hit.path).as_deref() {
            Some("pdf") => "application-pdf",
            Some("png") | Some("jpg") | Some("jpeg") | Some("webp") | Some("gif") | Some("svg") => {
                "image-x-generic"
            }
            Some("rs") | Some("c") | Some("cc") | Some("cpp") | Some("h") | Some("hpp")
            | Some("py") | Some("js") | Some("ts") | Some("tsx") | Some("jsx") | Some("go")
            | Some("java") | Some("kt") | Some("swift") | Some("rb") | Some("php") | Some("sh") => {
                "text-x-script"
            }
            Some("md") | Some("markdown") | Some("txt") => "text-x-generic",
            Some("json") | Some("toml") | Some("yaml") | Some("yml") => "text-x-generic",
            Some("zip") | Some("gz") | Some("tar") | Some("xz") | Some("7z") => "package-x-generic",
            _ => match hit.kind.as_str() {
                "document" => "x-office-document",
                "code" => "text-x-script",
                "image" => "image-x-generic",
                _ => "text-x-generic",
            },
        }
    }

    fn file_extension(path: &str) -> Option<String> {
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
    }

    fn desktop_icon_name(path: &str) -> Option<String> {
        let bytes = fs::read(path).ok()?;
        for line in String::from_utf8_lossy(&bytes[..bytes.len().min(128 * 1024)]).lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some(icon) = line.strip_prefix("Icon=") else {
                continue;
            };
            let icon = icon.trim();
            if !icon.is_empty() {
                return Some(icon.to_string());
            }
        }
        None
    }

    fn append_hint_row(results: &gtk::ListBox, text: &str) {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("search-result-row");
        let label = gtk::Label::new(Some(text));
        label.add_css_class("search-result-subtitle");
        label.set_xalign(0.0);
        row.set_child(Some(&label));
        results.append(&row);
    }

    fn replace_hint_row(results: &gtk::ListBox, text: &str) {
        clear_rows(results);
        append_hint_row(results, text);
    }

    fn clear_rows(results: &gtk::ListBox) {
        while let Some(row) = results.row_at_index(0) {
            results.remove(&row);
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum GlassLayerKind {
        Input,
        Results,
    }

    #[derive(Debug, Clone, Copy)]
    struct LiquidGlassPaint {
        kind: GlassLayerKind,
        radius: f64,
        tint_alpha: f64,
        border_alpha: f64,
        shadow_alpha: f64,
        highlight_alpha: f64,
        refraction: f64,
        chroma: f64,
        noise: f64,
        palette: GlassPalette,
    }

    #[derive(Debug, Clone, Copy)]
    struct GlassPalette {
        tint: (f64, f64, f64),
        accent: (f64, f64, f64),
        accent_alt: (f64, f64, f64),
        line_bias: f64,
    }

    fn liquid_glass_layer(
        config: &visionclip_common::config::SearchOverlayConfig,
        kind: GlassLayerKind,
    ) -> gtk::DrawingArea {
        let style =
            visionclip_common::config::normalize_search_overlay_glass_style(&config.glass_style);
        let paint = LiquidGlassPaint {
            kind,
            radius: f64::from(config.corner_radius_px.clamp(8, 40)),
            tint_alpha: f64::from(config.panel_opacity.clamp(0.0, 1.0)),
            border_alpha: f64::from(config.border_opacity.clamp(0.0, 1.0)),
            shadow_alpha: f64::from(config.shadow_intensity.clamp(0.0, 1.0)),
            highlight_alpha: f64::from(config.highlight_intensity.clamp(0.0, 1.0)),
            refraction: f64::from(config.refraction_strength.clamp(0.0, 1.0)),
            chroma: f64::from(config.chromatic_aberration.clamp(0.0, 1.0)),
            noise: f64::from(config.liquid_noise.clamp(0.0, 1.0)),
            palette: glass_palette(&style, config),
        };
        let height = match kind {
            GlassLayerKind::Input => 84,
            GlassLayerKind::Results => 96,
        };
        let area = gtk::DrawingArea::builder()
            .content_height(height)
            .hexpand(true)
            .vexpand(matches!(kind, GlassLayerKind::Results))
            .build();
        area.add_css_class("liquid-glass-canvas");

        let started_at = Instant::now();
        area.set_draw_func(move |_, cr, width, height| {
            draw_liquid_glass_layer(cr, width as f64, height as f64, started_at.elapsed(), paint);
        });
        area
    }

    fn glass_palette(
        style: &str,
        config: &visionclip_common::config::SearchOverlayConfig,
    ) -> GlassPalette {
        let primary = normalized_rgb(&config.primary).unwrap_or((0.23, 0.51, 0.96));
        let secondary = normalized_rgb(&config.secondary).unwrap_or((0.55, 0.36, 0.96));
        let ai = normalized_rgb(&config.ai_glow).unwrap_or((0.18, 0.85, 0.96));

        match style {
            "aurora_gel" => GlassPalette {
                tint: (0.32, 0.92, 0.78),
                accent: (0.62, 0.35, 1.0),
                accent_alt: (0.12, 0.95, 0.86),
                line_bias: 1.15,
            },
            "crystal_mist" | "frost_lens" => GlassPalette {
                tint: (0.88, 0.96, 1.0),
                accent: (0.75, 0.90, 1.0),
                accent_alt: (1.0, 1.0, 1.0),
                line_bias: 0.85,
            },
            "fluid_amber" | "molten_glass" => GlassPalette {
                tint: (1.0, 0.64, 0.24),
                accent: (1.0, 0.50, 0.12),
                accent_alt: (1.0, 0.88, 0.52),
                line_bias: 1.05,
            },
            "ice_ripple" => GlassPalette {
                tint: (0.70, 0.92, 1.0),
                accent: (0.38, 0.78, 1.0),
                accent_alt: (0.92, 1.0, 1.0),
                line_bias: 1.30,
            },
            "mercury_drop" => GlassPalette {
                tint: (0.78, 0.80, 0.86),
                accent: (0.93, 0.95, 1.0),
                accent_alt: (0.52, 0.55, 0.62),
                line_bias: 0.70,
            },
            "nebula_prism" | "prisma_flow" => GlassPalette {
                tint: (0.52, 0.32, 1.0),
                accent: (0.95, 0.34, 0.86),
                accent_alt: (0.20, 0.82, 1.0),
                line_bias: 1.25,
            },
            "ocean_wave" => GlassPalette {
                tint: (0.10, 0.72, 0.88),
                accent: (0.12, 0.78, 1.0),
                accent_alt: (0.04, 0.95, 0.72),
                line_bias: 1.35,
            },
            "plasma_flow" => GlassPalette {
                tint: (0.95, 0.24, 0.96),
                accent: (1.0, 0.30, 0.58),
                accent_alt: (0.44, 0.36, 1.0),
                line_bias: 1.35,
            },
            "silk_veil" => GlassPalette {
                tint: (1.0, 0.86, 0.94),
                accent: (1.0, 0.78, 0.92),
                accent_alt: (0.86, 0.92, 1.0),
                line_bias: 0.78,
            },
            "color_shifted" | "vibrant" | "liquid_glass_advanced" | "animated_glass" => {
                GlassPalette {
                    tint: primary,
                    accent: secondary,
                    accent_alt: ai,
                    line_bias: 1.15,
                }
            }
            _ => GlassPalette {
                tint: (0.95, 0.98, 1.0),
                accent: primary,
                accent_alt: ai,
                line_bias: 1.0,
            },
        }
    }

    fn draw_liquid_glass_layer(
        cr: &cairo::Context,
        width: f64,
        height: f64,
        elapsed: Duration,
        paint: LiquidGlassPaint,
    ) {
        if width <= 2.0 || height <= 2.0 {
            return;
        }

        let t = elapsed.as_secs_f64();
        let radius = paint.radius.min(width / 2.0).min(height / 2.0);

        cr.save().ok();
        rounded_rect(cr, 0.75, 0.75, width - 1.5, height - 1.5, radius);
        cr.clip();

        draw_lens_tint(cr, width, height, paint);
        draw_liquid_distortion_hint(cr, width, height, t, paint);
        draw_caustic_highlights(cr, width, height, t, paint);
        draw_internal_depth(cr, width, height, paint);

        cr.restore().ok();

        draw_lens_edge(cr, width, height, radius, paint);
        draw_lens_shadow(cr, width, height, radius, paint);
    }

    fn draw_lens_tint(cr: &cairo::Context, width: f64, height: f64, paint: LiquidGlassPaint) {
        let base_alpha = match paint.kind {
            GlassLayerKind::Input => paint.tint_alpha.min(0.10),
            GlassLayerKind::Results => (paint.tint_alpha + 0.03).min(0.14),
        };
        let (tr, tg, tb) = paint.palette.tint;
        cr.set_source_rgba(tr, tg, tb, base_alpha);
        cr.paint().ok();

        let sheen = cairo::LinearGradient::new(0.0, 0.0, width, height);
        sheen.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, 0.16 * paint.highlight_alpha);
        sheen.add_color_stop_rgba(0.42, 1.0, 1.0, 1.0, 0.015);
        sheen.add_color_stop_rgba(1.00, tr, tg, tb, 0.05 * paint.shadow_alpha);
        cr.set_source(&sheen).ok();
        cr.paint().ok();
    }

    fn draw_liquid_distortion_hint(
        cr: &cairo::Context,
        width: f64,
        height: f64,
        t: f64,
        paint: LiquidGlassPaint,
    ) {
        let columns = 9;
        let rows = 5;
        let alpha = (0.035 + paint.noise * 0.055) * paint.palette.line_bias;
        let (ar, ag, ab) = paint.palette.accent;
        let (br, bg, bb) = paint.palette.accent_alt;
        for y in 0..rows {
            let yy = height * (y as f64 + 0.5) / rows as f64;
            for x in 0..columns {
                let xx = width * (x as f64 + 0.5) / columns as f64;
                let phase = t * 0.45 + x as f64 * 0.73 + y as f64 * 1.17;
                let rx = width * (0.055 + 0.018 * phase.sin().abs());
                let ry = height * (0.10 + 0.035 * phase.cos().abs());
                let gradient = cairo::RadialGradient::new(xx, yy, 0.0, xx, yy, rx.max(ry));
                gradient.add_color_stop_rgba(0.0, ar, ag, ab, alpha * phase.sin().abs());
                gradient.add_color_stop_rgba(0.38, br, bg, bb, alpha * 0.32);
                gradient.add_color_stop_rgba(0.58, 1.0, 1.0, 1.0, alpha * 0.20);
                gradient.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
                cr.save().ok();
                cr.translate(xx, yy);
                cr.rotate(phase.sin() * 0.8);
                cr.scale(1.0, (ry / rx.max(1.0)).clamp(0.35, 2.2));
                cr.arc(0.0, 0.0, rx, 0.0, TAU);
                cr.set_source(&gradient).ok();
                cr.fill().ok();
                cr.restore().ok();
            }
        }
    }

    fn draw_caustic_highlights(
        cr: &cairo::Context,
        width: f64,
        height: f64,
        t: f64,
        paint: LiquidGlassPaint,
    ) {
        let line_alpha = (0.05 + paint.refraction * 0.11) * paint.palette.line_bias;
        let (ar, ag, ab) = paint.palette.accent;
        let (br, bg, bb) = paint.palette.accent_alt;
        for idx in 0..5 {
            let y = height * (0.18 + idx as f64 * 0.15);
            let offset = (t * (20.0 + idx as f64 * 4.0) + idx as f64 * 37.0).rem_euclid(width);
            cr.save().ok();
            cr.set_line_width(1.0 + paint.refraction * 1.2);
            cr.set_line_cap(cairo::LineCap::Round);
            if idx % 2 == 0 {
                cr.set_source_rgba(ar, ag, ab, line_alpha * (1.0 - idx as f64 * 0.10));
            } else {
                cr.set_source_rgba(br, bg, bb, line_alpha * (1.0 - idx as f64 * 0.10));
            }
            cr.new_path();
            cr.move_to(-width + offset, y);
            let segments = 8;
            for step in 0..=segments {
                let x = -width + offset + width * 2.0 * step as f64 / segments as f64;
                let wave = ((step as f64 * 1.9) + t * 1.2 + idx as f64).sin();
                let control_y = y + wave * height * 0.10;
                let next_x = x + width / segments as f64;
                let next_y =
                    y + ((step as f64 * 1.9) + t * 1.2 + idx as f64 + 0.8).sin() * height * 0.08;
                cr.curve_to(
                    x + width / 24.0,
                    control_y,
                    next_x - width / 24.0,
                    next_y,
                    next_x,
                    next_y,
                );
            }
            cr.stroke().ok();
            cr.restore().ok();
        }

        let chroma = paint.chroma;
        if chroma > 0.01 {
            cr.save().ok();
            cr.set_line_width(1.0);
            rounded_rect(cr, 2.0, 2.0, width - 4.0, height - 4.0, paint.radius - 1.5);
            cr.set_source_rgba(ar, ag, ab, 0.13 * chroma);
            cr.stroke().ok();
            rounded_rect(cr, 3.0, 3.0, width - 6.0, height - 6.0, paint.radius - 2.5);
            cr.set_source_rgba(br, bg, bb, 0.10 * chroma);
            cr.stroke().ok();
            cr.restore().ok();
        }
    }

    fn draw_internal_depth(cr: &cairo::Context, width: f64, height: f64, paint: LiquidGlassPaint) {
        let top = cairo::LinearGradient::new(0.0, 0.0, 0.0, height * 0.35);
        top.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.20 * paint.highlight_alpha);
        top.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
        cr.set_source(&top).ok();
        cr.rectangle(0.0, 0.0, width, height * 0.35);
        cr.fill().ok();

        let bottom = cairo::LinearGradient::new(0.0, height * 0.65, 0.0, height);
        bottom.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
        bottom.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.16 * paint.shadow_alpha);
        cr.set_source(&bottom).ok();
        cr.rectangle(0.0, height * 0.65, width, height * 0.35);
        cr.fill().ok();
    }

    fn draw_lens_edge(
        cr: &cairo::Context,
        width: f64,
        height: f64,
        radius: f64,
        paint: LiquidGlassPaint,
    ) {
        cr.save().ok();
        rounded_rect(cr, 0.75, 0.75, width - 1.5, height - 1.5, radius);
        cr.set_line_width(1.0);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.18 + paint.border_alpha * 0.34);
        cr.stroke().ok();

        rounded_rect(cr, 2.0, 2.0, width - 4.0, height - 4.0, radius - 1.5);
        cr.set_line_width(1.0);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.05 + paint.highlight_alpha * 0.10);
        cr.stroke().ok();
        cr.restore().ok();
    }

    fn draw_lens_shadow(
        cr: &cairo::Context,
        width: f64,
        height: f64,
        radius: f64,
        paint: LiquidGlassPaint,
    ) {
        cr.save().ok();
        rounded_rect(cr, 1.0, 1.0, width - 2.0, height - 2.0, radius);
        cr.set_line_width(10.0);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.03 + paint.shadow_alpha * 0.05);
        cr.stroke().ok();
        cr.restore().ok();
    }

    fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
        let radius = radius.max(0.0).min(width / 2.0).min(height / 2.0);
        cr.new_path();
        cr.arc(x + width - radius, y + radius, radius, -TAU / 4.0, 0.0);
        cr.arc(
            x + width - radius,
            y + height - radius,
            radius,
            0.0,
            TAU / 4.0,
        );
        cr.arc(
            x + radius,
            y + height - radius,
            radius,
            TAU / 4.0,
            TAU / 2.0,
        );
        cr.arc(x + radius, y + radius, radius, TAU / 2.0, TAU * 0.75);
        cr.close_path();
    }

    fn install_css(config: &visionclip_common::config::SearchOverlayConfig) {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(&overlay_css(config));

        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn overlay_css(config: &visionclip_common::config::SearchOverlayConfig) -> String {
        let style = if config.liquid_glass_enabled {
            visionclip_common::config::normalize_search_overlay_glass_style(&config.glass_style)
        } else {
            "dark_glass".to_string()
        };
        let ai = hex_to_rgb(&config.ai_glow).unwrap_or((47, 217, 244));

        let radius = config.corner_radius_px.clamp(8, 40);
        let results_radius = radius.saturating_sub(4).max(8);

        let animation = if config.animations_enabled {
            "ai-processing 2s ease infinite"
        } else {
            "none"
        };

        let (text_pri, text_sec, icon_col) = match style.as_str() {
            "glass" | "glassmorphism" | "frosted" | "bright_overlay" | "inverted" => (
                "#1c1c1e".to_string(),
                "#5f6368".to_string(),
                "#2c2c2e".to_string(),
            ),
            "high_contrast" | "accessible_glass" => (
                "#ffffff".to_string(),
                "#f4f4f5".to_string(),
                "#ffffff".to_string(),
            ),
            "desaturated" | "monochrome" => (
                "#f0f0f2".to_string(),
                "#c8c8ce".to_string(),
                "#f6f6f8".to_string(),
            ),
            "vintage" => (
                "#fff2df".to_string(),
                "#dfc9af".to_string(),
                "#ffe0a8".to_string(),
            ),
            "neumorphism"
            | "neumorphic_pressed"
            | "neumorphic_concave"
            | "neumorphic_colored"
            | "neumorphic_accessible" => (
                "#22252c".to_string(),
                "#5f6672".to_string(),
                "#353945".to_string(),
            ),
            "liquid_crystal"
            | "liquid_glass"
            | "liquid_glass_advanced"
            | "animated_glass"
            | "vibrant"
            | "color_shifted"
            | "dark_glass"
            | "dark_overlay" => (
                "#ffffff".to_string(),
                "#e0e6ed".to_string(),
                "#ffffff".to_string(),
            ),
            _ => (
                "#ffffff".to_string(),
                "#e0e6ed".to_string(),
                "#ffffff".to_string(),
            ),
        };

        let (ar, ag, ab) = ai;

        format!(
            r#"
            @keyframes ai-processing {{
                0% {{ background-position: 0% 50%; }}
                50% {{ background-position: 100% 50%; }}
                100% {{ background-position: 0% 50%; }}
            }}

            window,
            window > contents,
            window.background,
            window.background > contents {{
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                box-shadow: none;
                border: none;
            }}

            .search-overlay-root {{
                margin: 22px;
                background: transparent;
            }}

            .search-input-panel {{
                border-radius: {radius}px;
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                background-image: none;
                border: none;
                box-shadow: none;
                color: {text_pri};
            }}

            .search-input-panel-open {{
                border-radius: {radius}px {radius}px {results_radius}px {results_radius}px;
            }}

            .search-results-panel {{
                margin-top: 8px;
                border-radius: {results_radius}px {results_radius}px {radius}px {radius}px;
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                background-image: none;
                border: none;
                box-shadow: none;
                color: {text_pri};
            }}

            .search-results-content {{
                border-radius: {results_radius}px {results_radius}px {radius}px {radius}px;
                overflow: hidden;
            }}

            .search-input-row {{
                min-height: 82px;
                padding: 0 24px;
                border-radius: {radius}px;
            }}

            .search-leading-icon {{
                min-width: 52px;
                min-height: 52px;
                padding: 0;
                margin: 0;
                background: transparent;
                color: {icon_col};
                font: 700 17px Inter, Sans;
            }}

            .search-leading-icon-frame {{
                min-width: 52px;
                min-height: 52px;
                border-radius: 999px;
                background-color: rgba(255,255,255,0.08);
                border: 1px solid rgba(255,255,255,0.10);
                box-shadow: inset 0 1px 1px rgba(255,255,255,0.18), 0 0 18px rgba(255,255,255,0.08);
            }}

            entry.search-entry,
            .search-entry {{
                min-height: 82px;
                background: transparent;
                color: {text_pri};
                caret-color: {primary};
                border: none;
                box-shadow: none;
                outline: none;
                font: 700 34px Inter, Sans;
            }}

            entry.search-entry:focus,
            entry.search-entry:focus-within,
            .search-entry:focus,
            .search-entry:focus-within {{
                background: transparent;
                border: none;
                box-shadow: none;
                outline: none;
            }}

            entry.search-entry > *,
            .search-entry > * {{
                background: transparent;
                border: none;
                box-shadow: none;
                outline: none;
            }}

            .search-entry text,
            .search-entry selection {{
                color: {text_pri};
            }}

            .ai-processing-bar {{
                min-height: 2px;
                background-image: linear-gradient(90deg, transparent, rgba({ar},{ag},{ab},0.78), rgba(255,255,255,0.72), rgba({ar},{ag},{ab},0.78), transparent);
                background-size: 300% 300%;
                box-shadow:
                    0 0 8px rgba({ar},{ag},{ab},0.42),
                    0 0 20px rgba(255,255,255,0.14);
                animation: {animation};
            }}

            scrolledwindow.search-results-scroll,
            .search-results-scroll {{
                margin: 0;
                padding: 0;
                border-radius: {results_radius}px {results_radius}px {radius}px {radius}px;
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                border: none;
                box-shadow: none;
                overflow: hidden;
            }}

            scrolledwindow.search-results-scroll > viewport,
            .search-results-scroll > viewport {{
                border-radius: {results_radius}px {results_radius}px {radius}px {radius}px;
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                border: none;
                box-shadow: none;
            }}

            scrolledwindow.search-results-scroll scrollbar.vertical {{
                min-width: 7px;
                margin: 12px 8px 12px 0;
                border-radius: 999px;
                background: rgba(255,255,255,0.05);
            }}

            scrolledwindow.search-results-scroll scrollbar.vertical slider {{
                min-height: 34px;
                border-radius: 999px;
                background: rgba({ar},{ag},{ab},0.42);
                box-shadow: 0 0 12px rgba({ar},{ag},{ab},0.32);
            }}

            .search-results {{
                padding: 8px;
                background: transparent;
            }}

            .search-result-row {{
                padding: 12px 14px;
                margin-bottom: 2px;
                border-radius: 12px;
                background: transparent;
                transition: 150ms ease;
            }}

            .search-result-row:hover {{
                background-color: rgba(255, 255, 255, 0.07);
            }}

            .search-result-row:selected {{
                background-color: rgba(255, 255, 255, 0.10);
                border-radius: 12px;
            }}

            .search-result-content {{
                min-height: 58px;
            }}

            .search-result-icon-frame {{
                min-width: 52px;
                min-height: 52px;
                border-radius: 26px;
                padding: 0;
                background-color: rgba(255,255,255,0.09);
                border: 1px solid rgba(255,255,255,0.10);
                box-shadow: inset 0 1px 0 rgba(255,255,255,0.14);
            }}

            .search-result-icon {{
                color: {icon_col};
            }}

            .search-result-title {{
                color: {text_pri};
                font: 600 15px Inter, Sans;
            }}

            .search-result-subtitle {{
                color: {text_sec};
                font: 400 12px Inter, Sans;
            }}

            .search-result-chip {{
                min-width: 70px;
                padding: 4px 9px;
                border-radius: 999px;
                background-color: rgba(255,255,255,0.13);
                color: #ffffff;
                font: 600 10px Inter, Sans;
            }}
            
            .liquid-glass-canvas {{
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                background-image: none;
            }}
            "#,
            radius = radius,
            results_radius = results_radius,
            text_pri = text_pri,
            text_sec = text_sec,
            icon_col = icon_col,
            primary = config.primary,
            ar = ar,
            ag = ag,
            ab = ab,
            animation = animation,
        )
    }

    fn hex_to_rgb(value: &str) -> Option<(u8, u8, u8)> {
        let raw = value.trim().strip_prefix('#').unwrap_or(value.trim());
        if raw.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&raw[0..2], 16).ok()?;
        let g = u8::from_str_radix(&raw[2..4], 16).ok()?;
        let b = u8::from_str_radix(&raw[4..6], 16).ok()?;
        Some((r, g, b))
    }

    fn normalized_rgb(value: &str) -> Option<(f64, f64, f64)> {
        let (r, g, b) = hex_to_rgb(value)?;
        Some((
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::{
            is_outside_overlay_content, overlay_window_height_for_results,
            results_content_height_for_count, INPUT_ONLY_WINDOW_HEIGHT, RESULTS_MAX_CONTENT_HEIGHT,
            RESULTS_MIN_CONTENT_HEIGHT, RESULTS_REVEAL_GAP_PX,
        };

        #[test]
        fn outside_click_detection_respects_safe_inner_area() {
            assert!(is_outside_overlay_content(760.0, 118.0, 20.0, 6.0, 60.0));
            assert!(is_outside_overlay_content(760.0, 118.0, 20.0, 754.0, 60.0));
            assert!(is_outside_overlay_content(760.0, 118.0, 20.0, 380.0, 8.0));
            assert!(!is_outside_overlay_content(760.0, 118.0, 20.0, 380.0, 60.0));
        }

        #[test]
        fn outside_click_detection_ignores_tiny_allocations() {
            assert!(!is_outside_overlay_content(20.0, 20.0, 20.0, 0.0, 0.0));
        }

        #[test]
        fn result_panel_height_is_bounded_for_scrollable_lists() {
            assert_eq!(
                results_content_height_for_count(0),
                RESULTS_MIN_CONTENT_HEIGHT
            );
            assert_eq!(
                results_content_height_for_count(100),
                RESULTS_MAX_CONTENT_HEIGHT
            );
        }

        #[test]
        fn result_window_height_tracks_bounded_content() {
            assert_eq!(
                overlay_window_height_for_results(100),
                INPUT_ONLY_WINDOW_HEIGHT + RESULTS_REVEAL_GAP_PX + RESULTS_MAX_CONTENT_HEIGHT
            );
        }
    }
}
