mod app;
mod config;
mod font;
mod model;
mod player;
mod store;
mod tasks;
mod thumbs;
mod ui;
mod yt;

use eframe::egui;

use crate::app::{App, Section, Startup, ToastKind};
use crate::config::Theme;
use crate::model::Feed;
use crate::ui::{accent, detail, settings, sidebar, toasts, toolbar, video_list};

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let startup = parse_args();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("magic-tape")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([860.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native("magic-tape", options, Box::new(move |cc| {
        Ok(Box::new(MagicTape::new(cc, startup)))
    }))
}

struct MagicTape {
    app: App,
    booted: bool,
    /// The theme the context is currently styled with. `visuals_for` is only
    /// applied when this changes, so it runs once per switch rather than once
    /// per frame.
    applied: Option<Theme>,
    /// The font and scale the context is currently using, for the same reason.
    applied_font: Option<(String, f32)>,
}

fn parse_args() -> Startup {
    let mut args = std::env::args().skip(1);
    let mut startup = Startup::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--search" | "-s" => startup.query = args.next(),
            "--feed" | "-f" => {
                startup.feed = match args.next().as_deref() {
                    Some("trending") => Some(Feed::Trending),
                    Some("recommended") | Some("foryou") => Some(Feed::Recommended),
                    _ => Some(Feed::Subscriptions),
                }
            }
            "--help" | "-h" => {
                println!(
                    "magic-tape\n\nUSAGE:\n    magic-tape [OPTIONS] [QUERY]\n\nOPTIONS:\n    -s, --search <QUERY>    open straight into a search\n    -f, --feed <FEED>      subscriptions | trending | recommended\n    -h, --help             print this help"
                );
                std::process::exit(0);
            }
            other => startup.query = Some(other.to_string()),
        }
    }
    startup
}

impl MagicTape {
    fn new(cc: &eframe::CreationContext<'_>, startup: Startup) -> Self {
        let mut app = App::new(&cc.egui_ctx, startup);
        app.list_view = app.config.list_view;
        let applied_font = Some((app.config.ui_font.clone(), app.config.ui_zoom));
        Self {
            app,
            booted: false,
            applied: None,
            applied_font,
        }
    }
}

fn visuals_for(theme: Theme) -> egui::Visuals {
    let mut visuals = match theme {
        Theme::Light => egui::Visuals::light(),
        Theme::Dark | Theme::System => egui::Visuals::dark(),
    };
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.widgets.noninteractive.bg_stroke.color =
        egui::Color32::from_gray(if theme == Theme::Light { 205 } else { 60 });
    visuals.selection.bg_fill = accent().gamma_multiply(0.35);
    visuals.hyperlink_color = accent();
    visuals
}

impl MagicTape {
    /// Reinstalls the UI font when the setting changes, and complains once if
    /// the chosen font cannot be read.
    fn apply_font_change(&mut self, ctx: &egui::Context) {
        let wanted = (self.app.config.ui_font.clone(), self.app.config.ui_zoom);
        if self.applied_font.as_ref() == Some(&wanted) {
            return;
        }
        let path = font::resolve(&wanted.0, font::discovered());
        if wanted.0.trim().is_empty() != path.is_none() {
            log::warn!(
                "[font] {:?} could not be matched to an installed font",
                wanted.0
            );
            self.app.toast(
                format!("No installed font matches \"{}\"", wanted.0),
                crate::app::ToastKind::Error,
            );
        }
        if let Err(error) = font::apply(ctx, path.as_deref(), wanted.1) {
            log::error!("[font] could not be applied: {error}");
            self.app
                .toast(format!("Font not applied: {error}"), crate::app::ToastKind::Error);
        }
        self.applied_font = Some(wanted);
    }
}

impl eframe::App for MagicTape {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for outcome in self.app.worker.drain() {
            self.app.handle_outcome(outcome);
        }
        self.app.prune_toasts();
        if !self.booted {
            self.booted = true;
            self.app.boot();
        }
        self.apply_font_change(ctx);
        if self.app.worker.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_keys(&ctx);

        let target = match self.app.config.theme {
            Theme::System => {
                if ctx.style_of(egui::Theme::Dark).visuals.dark_mode {
                    Theme::Dark
                } else {
                    Theme::Light
                }
            }
            explicit => explicit,
        };
        if self.applied != Some(target) {
            ctx.set_visuals(visuals_for(target));
            self.applied = Some(target);
        }
        let target_dark = target == Theme::Dark;

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(236.0)
            .min_size(196.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| sidebar::sidebar(&mut self.app, ui));
            });

        detail::detail_panel(&mut self.app, &ctx, ui);

        egui::Panel::bottom("status")
            .exact_size(26.0)
            .show(ui, |ui| toasts::status_bar(&self.app, ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(if target_dark {
                egui::Color32::from_rgb(18, 18, 22)
            } else {
                egui::Color32::from_rgb(240, 240, 243)
            }))
            .show(ui, |ui| {
                ui.add_space(6.0);
                toolbar::toolbar(&mut self.app, ui);
                ui.separator();
                video_list::content(&mut self.app, ui);
            });

        settings::settings_window(&mut self.app, &ctx);
        settings::shortcuts_window(&mut self.app, &ctx);
        toasts::render(&mut self.app, &ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if let Err(error) = self.app.save_config() {
            log::warn!("could not save config: {error}");
        }
    }
}

impl MagicTape {
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused()).is_some();
        let mut command = None;
        ctx.input(|i| {
            for event in &i.events {
                let egui::Event::Key { key, pressed: true, .. } = event else {
                    continue;
                };
                if i.modifiers.ctrl {
                    match key {
                        egui::Key::Q => command = Some(Command::Quit),
                        egui::Key::F => command = Some(Command::FocusSearch),
                        egui::Key::Comma => command = Some(Command::Settings),
                        egui::Key::ShiftLeft | egui::Key::ShiftRight => {}
                        _ => {}
                    }
                } else if i.modifiers.shift {
                    match key {
                        egui::Key::D => command = Some(Command::DownloadAudio),
                        egui::Key::ArrowDown => command = Some(Command::PageDown),
                        egui::Key::ArrowUp => command = Some(Command::PageUp),
                        _ => {}
                    }
                } else {
                    match key {
                        egui::Key::ArrowDown | egui::Key::J => command = Some(Command::MoveDown),
                        egui::Key::ArrowUp | egui::Key::K => command = Some(Command::MoveUp),
                        egui::Key::PageDown => command = Some(Command::PageDown),
                        egui::Key::PageUp => command = Some(Command::PageUp),
                        egui::Key::Home => command = Some(Command::First),
                        egui::Key::End => command = Some(Command::Last),
                        egui::Key::Enter => command = Some(Command::Play),
                        egui::Key::F => command = Some(Command::PlayBest),
                        egui::Key::A => command = Some(Command::PlayAudio),
                        egui::Key::D => command = Some(Command::DownloadVideo),
                        egui::Key::L => command = Some(Command::Like),
                        egui::Key::S => command = Some(Command::Subscribe),
                        egui::Key::B => command = Some(Command::Browser),
                        egui::Key::C => command = Some(Command::CopyLink),
                        egui::Key::R => command = Some(Command::Refresh),
                        egui::Key::Comma | egui::Key::P => command = Some(Command::PrevPage),
                        egui::Key::Period | egui::Key::N => command = Some(Command::NextPage),
                        _ => {}
                    }
                }
            }
        });
        if typing && !matches!(command, Some(Command::Quit))
            && !matches!(
                command,
                Some(Command::MoveDown) | Some(Command::MoveUp) | Some(Command::Quit)
            ) {
                return;
            }
        match command {
            Some(Command::Quit) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Some(Command::Settings) => self.app.show_settings = true,
            Some(Command::FocusSearch) => self.app.search_input.clear(),
            Some(Command::MoveDown) => self.move_selection(1),
            Some(Command::MoveUp) => self.move_selection(-1),
            Some(Command::PageDown) => {
                let step = self.app.page_step() as isize;
                self.move_selection(step);
            }
            Some(Command::PageUp) => {
                let step = self.app.page_step() as isize;
                self.move_selection(-step);
            }
            Some(Command::First) => self.select_edge(true),
            Some(Command::Last) => self.select_edge(false),
            Some(Command::Play) => self.app.play_selected("1080p"),
            Some(Command::PlayBest) => self.app.play_selected("best"),
            Some(Command::PlayAudio) => self.app.play_selected("audio"),
            Some(Command::DownloadVideo) => self.app.download_selected(false),
            Some(Command::DownloadAudio) => self.app.download_selected(true),
            Some(Command::Like) => self.app.toggle_like(),
            Some(Command::Subscribe) => self.app.subscribe_to_current(),
            Some(Command::Browser) => {
                if let Some(video) = self.app.current_video()
                    && let Err(error) = player::open_browser(&video.url) {
                        self.app.toast(error.to_string(), ToastKind::Error);
                    }
            }
            Some(Command::CopyLink) => {
                if let Some(video) = self.app.current_video() {
                    ctx.copy_text(video.url.clone());
                    self.app.toast("Link copied to clipboard".into(), ToastKind::Success);
                }
            }
            Some(Command::Refresh) => self.refresh(),
            Some(Command::PrevPage) => self.page(-1),
            Some(Command::NextPage) => self.page(1),
            None => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.app.item_count();
        if count == 0 {
            return;
        }
        let current = self.app.selected.unwrap_or(0);
        if delta > 0
            && current + delta as usize >= count
            && self.app.has_more()
            && !self.app.feed_state.loading
        {
            let first_new = count;
            self.app.pending_select = Some(first_new);
            log::info!(
                "[nav] reached the end of the loaded items, loading the next page (from {first_new})"
            );
            self.app.load_next_page();
            return;
        }
        if let Some(next) = App::clamp_index(self.app.selected, count, delta) {
            self.app.select(Some(next));
        }
    }

    fn select_edge(&mut self, first: bool) {
        let count = self.app.item_count();
        if count == 0 {
            return;
        }
        self.app.select(Some(if first { 0 } else { count - 1 }));
    }

    fn refresh(&mut self) {
        match self.app.section {
            Section::Feeds => self.app.load_feed(self.app.feed, 0),
            Section::Playlists => {
                let query = self.app.playlist_query.clone();
                self.app.load_playlists(&query, 0)
            }
            Section::Channels => {
                let query = self.app.channel_query.clone();
                self.app.search_channels(&query, 0)
            }
            Section::Liked => self.app.liked = self.app.store.liked(),
            Section::History => {
                self.app.history = self.app.store.watch_history();
                self.app.history.reverse();
            }
        }
    }

    fn page(&mut self, direction: isize) {
        if self.app.section != Section::Feeds {
            return;
        }
        if direction > 0 {
            self.app.load_next_page();
        } else {
            self.app.load_prev_page();
        }
    }
}

enum Command {
    Quit,
    Settings,
    FocusSearch,
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    First,
    Last,
    Play,
    PlayBest,
    PlayAudio,
    DownloadVideo,
    DownloadAudio,
    Like,
    Subscribe,
    Browser,
    CopyLink,
    Refresh,
    PrevPage,
    NextPage,
}

#[allow(dead_code)]
fn feed_label(feed: Feed) -> &'static str {
    feed.label()
}