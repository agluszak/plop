use bevy::app::AppExit;
use bevy::audio::{PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use bevy_prng::WyRand;
use bevy_rand::prelude::*;
use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2, containers::Scene};
use plop::{AppState, Board, NoteData, snap_to_grid};
use rand::Rng;
use std::path::PathBuf;

/// Runtime UI state for a note
#[derive(Component)]
struct NoteUi {
    is_editing: bool,
    /// Current skew applied while dragging for a leaning effect
    skew: Vec2,
}

impl Default for NoteUi {
    fn default() -> Self {
        Self {
            is_editing: false,
            skew: Vec2::ZERO,
        }
    }
}


// Audio resource to play the plop sound
#[derive(Resource)]
struct AudioAssets {
    plop: Handle<AudioSource>,
}

/// Grid size controlling note alignment
#[derive(Resource)]
struct GridSize(f32);

impl Default for GridSize {
    fn default() -> Self {
        Self(50.0)
    }
}

// Bevy resource to hold our app state
#[derive(Resource)]
struct PostItData {
    state: AppState,
    save_path: PathBuf,
}

impl Default for PostItData {
    fn default() -> Self {
        // Where to persist JSON
        let mut save_path = dirs::home_dir().unwrap_or_default();
        save_path.push("egui_postit_state.json");

        // Load existing state or start fresh
        let state = AppState::load_from_file(&save_path);

        Self { state, save_path }
    }
}

// Store which board needs sound played in events
#[derive(Event, Default)]
struct PlayPlopEvent;

#[derive(Resource, Default)]
struct SearchState {
    query: String,
    matches: Vec<u64>, // note_id
    current: usize,
}

fn update_search(app: &PostItData, search: &mut SearchState) {
    search.matches.clear();
    if search.query.is_empty() {
        return;
    }
    let q = search.query.to_lowercase();
    for note in &app.state.board.notes {
        if note.text.to_lowercase().contains(&q) {
            search.matches.push(note.id);
        }
    }
    search.current = 0;
}

fn focus_on_match(app: &mut PostItData, search: &SearchState) {
    if let Some(&nid) = search.matches.get(search.current) {
        if let Some(note) = app.state.board.notes.iter().find(|n| n.id == nid) {
            let center = Pos2::new(
                note.pos.x + note.size.x / 2.0,
                note.pos.y + note.size.y / 2.0,
            );
            app.state.board.scene_rect =
                Rect::from_center_size(center, app.state.board.scene_rect.size());
        }
    }
}

// System to handle plop sound events
fn play_plop_sound(
    audio_assets: Res<AudioAssets>,
    mut commands: Commands,
    mut events: EventReader<PlayPlopEvent>,
    mut rng: GlobalEntropy<WyRand>,
) {
    for _ in events.read() {
        // Randomize speed and volume slightly for variety
        let speed = rng.gen_range(0.9..=1.1);
        let volume = rng.gen_range(0.8..=1.2);
        commands.spawn((
            AudioPlayer::new(audio_assets.plop.clone()),
            PlaybackSettings::DESPAWN
                .with_speed(speed)
                .with_volume(Volume::Linear(volume)),
        ));
    }
}

/// Calculate a font size so the text fits inside the note rectangle
fn fitted_font_size(ctx: &egui::Context, text: &str, max: Vec2, start: f32) -> f32 {
    let mut size = start;
    let margin = 8.0;
    while size > 6.0 {
        let font_id = egui::FontId::proportional(size);
        let galley = ctx.fonts(|f| f.layout_no_wrap(text.to_owned(), font_id, Color32::BLACK));
        let text_size = galley.size();
        if text_size.x <= max.x - margin && text_size.y <= max.y - margin {
            break;
        }
        size -= 1.0;
    }
    size.max(6.0)
}

fn highlighted_layout(text: &str, query: &str, font_size: f32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let normal = TextFormat::simple(egui::FontId::proportional(font_size), Color32::BLACK);
    let mut highlight = normal.clone();
    highlight.background = Color32::LIGHT_RED;
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    let mut i = 0;
    while let Some(pos) = text_lower[i..].find(&query_lower) {
        let start = i + pos;
        if start > i {
            job.append(&text[i..start], 0.0, normal.clone());
        }
        let end = start + query.len();
        job.append(&text[start..end], 0.0, highlight.clone());
        i = end;
    }
    if i < text.len() {
        job.append(&text[i..], 0.0, normal);
    }
    job
}

fn ui_system(
    mut commands: Commands,
    mut app: ResMut<PostItData>,
    mut contexts: EguiContexts,
    mut ev_plop: EventWriter<PlayPlopEvent>,
    grid: Res<GridSize>,
    mut notes: Query<(Entity, &mut NoteData, &mut NoteUi)>,
    mut search: ResMut<SearchState>,
) {
    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Save/Load controls
            if ui.button("Save").clicked() {
                // Sync notes from ECS into the app state before saving
                for (_, note, _) in notes.iter_mut() {
                    if let Some(n) = app.state.board.notes.iter_mut().find(|n| n.id == note.id) {
                        *n = note.clone();
                    }
                }
                app.state.save_to_file(&app.save_path);
            }
            if ui.button("Load").clicked() {
                app.state = AppState::load_from_file(&app.save_path);
                // Remove existing note entities
                for (e, _, _) in notes.iter_mut() {
                    commands.entity(e).despawn();
                }
                // Spawn notes from loaded state
                for note in &app.state.board.notes {
                    commands.spawn((note.clone(), NoteUi::default()));
                }
                update_search(&app, &mut search);
            }

            ui.separator();
            ui.label("Search:");
            let changed = ui.text_edit_singleline(&mut search.query).changed();
            if changed {
                update_search(&app, &mut search);
                focus_on_match(&mut app, &search);
            }
            if ui.button("Prev").clicked() && !search.matches.is_empty() {
                if search.current == 0 {
                    search.current = search.matches.len() - 1;
                } else {
                    search.current -= 1;
                }
                focus_on_match(&mut app, &search);
            }
            if ui.button("Next").clicked() && !search.matches.is_empty() {
                search.current = (search.current + 1) % search.matches.len();
                focus_on_match(&mut app, &search);
            }
        });
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        let mut next_id = app.state.next_note_id;
        let highlight = search.matches.get(search.current).copied();
        board_ui_system(
            ui,
            &mut app.state.board,
            &mut next_id,
            &mut notes,
            &mut commands,
            &grid,
            &mut ev_plop,
            &search.query,
            highlight,
        );
        app.state.next_note_id = next_id;
    });
}

/// Render a single board: background + draggable notes
fn board_ui_system(
    ui: &mut egui::Ui,
    board: &mut Board,
    next_note_id: &mut u64,
    notes: &mut Query<(Entity, &mut NoteData, &mut NoteUi)>,
    commands: &mut Commands,
    grid: &GridSize,
    ev_plop: &mut EventWriter<PlayPlopEvent>,
    query: &str,
    highlight_note: Option<u64>,
) {
    // Zoomable + draggable scene
    let scene = Scene::new()
        .zoom_range(0.1..=5.0)
        .max_inner_size(Vec2::splat(5000.0));
    let mut scene_rect = board.scene_rect;
    let response = scene
        .show(ui, &mut scene_rect, |ui| {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, board.background);

            // Render existing notes from ECS
            for (_, mut note, mut ui_state) in notes.iter_mut() {
                let highlight = highlight_note == Some(note.id);
                let has_query =
                    !query.is_empty() && note.text.to_lowercase().contains(&query.to_lowercase());
                add_note_ui(
                    ui,
                    &mut note,
                    &mut ui_state,
                    board,
                    grid.0,
                    ev_plop,
                    query,
                    has_query,
                    highlight,
                );
            }
        })
        .response;
    board.scene_rect = scene_rect;

    // If user right-clicks on the board, add new note
    if response.hovered()
        && ui
            .ctx()
            .input(|i| i.pointer.button_released(egui::PointerButton::Secondary))
    {
        let id = *next_note_id;
        *next_note_id += 1;
        let pointer_pos = ui
            .ctx()
            .pointer_hover_pos()
            .unwrap_or(Pos2 { x: 0.0, y: 0.0 });
        let data = NoteData {
            id,
            text: "New note".into(),
            pos: snap_to_grid(pointer_pos, grid.0),
            size: Vec2 { x: 120.0, y: 80.0 },
            color: Color32::YELLOW,
        };
        commands.spawn((data.clone(), NoteUi::default()));
        board.notes.push(data);

        // Send event to play sound
        ev_plop.write_default();
    }
}

/// Draw one note; drag-handling + wiggle
fn add_note_ui(
    ui: &mut egui::Ui,
    note: &mut NoteData,
    ui_state: &mut NoteUi,
    board: &mut Board,
    grid_size: f32,
    ev_plop: &mut EventWriter<PlayPlopEvent>,
    query: &str,
    highlight_match: bool,
    active: bool,
) {
    // Allocate interaction area based on the original note size
    let base_rect = Rect::from_min_size(note.pos, note.size);
    let response = ui.allocate_rect(base_rect, egui::Sense::click_and_drag());

    if response.double_clicked() {
        ui_state.is_editing = true;
    }

    if ui_state.is_editing {
        egui::Window::new(format!("edit_note_{}", note.id))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .fixed_pos(note.pos)
            .show(ui.ctx(), |ui| {
                ui.add(egui::TextEdit::multiline(&mut note.text).desired_width(note.size.x - 10.0));
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    ui.color_edit_button_srgba(&mut note.color);
                });
                if ui.button("Done").clicked() {
                    ui_state.is_editing = false;
                }
            });
        if let Some(n) = board.notes.iter_mut().find(|n| n.id == note.id) {
            n.text = note.text.clone();
            n.color = note.color;
        }
        return;
    }

    if response.dragged() {
        // Wiggle offset combined with stretchy scaling for a satisfying drag
        let t = ui.ctx().input(|i| i.time as f32);
        let wiggle_amp = 3.0;
        let wiggle_off = wiggle_amp * (t * 15.0).sin();

        let delta = response.drag_delta();
        note.pos.x += delta.x;
        note.pos.y += delta.y;
        if let Some(n) = board.notes.iter_mut().find(|n| n.id == note.id) {
            n.pos = note.pos;
        }

        // Update temporary skew based on drag speed
        let skew_factor = 0.02;
        let target_skew_x = delta.x * skew_factor;
        let target_skew_y = delta.y * skew_factor;
        ui_state.skew.x += (target_skew_x - ui_state.skew.x) * 0.5;
        ui_state.skew.y += (target_skew_y - ui_state.skew.y) * 0.5;

        let w = note.size.x;
        let h = note.size.y;
        let sx = ui_state.skew.x;
        let sy = ui_state.skew.y;
        let off = egui::vec2(wiggle_off, 0.0);

        let p1 = note.pos + off;
        let p2 = Pos2 {
            x: note.pos.x + w + off.x,
            y: note.pos.y + w * sy + off.y,
        };
        let p3 = Pos2 {
            x: note.pos.x + w + h * sx + off.x,
            y: note.pos.y + h + w * sy + off.y,
        };
        let p4 = Pos2 {
            x: note.pos.x + h * sx + off.x,
            y: note.pos.y + h + off.y,
        };

        let center = Pos2::new(
            (p1.x + p2.x + p3.x + p4.x) / 4.0,
            (p1.y + p2.y + p3.y + p4.y) / 4.0,
        );

        ui.painter().add(Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            note.color,
            Stroke::NONE,
        ));
        let font_size = fitted_font_size(ui.ctx(), &note.text, note.size, 16.0);
        if highlight_match {
            let job = highlighted_layout(&note.text, query, font_size);
            let galley = ui.painter().layout_job(job);
            ui.painter()
                .galley(center - galley.size() * 0.5, galley, Color32::BLACK);
        } else {
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                &note.text,
                egui::FontId::proportional(font_size),
                Color32::BLACK,
            );
        }

        // Draw preview of snapped position
        let snapped = snap_to_grid(note.pos, grid_size);
        let preview = Rect::from_min_size(snapped, note.size);
        ui.painter().rect_stroke(
            preview,
            0.0,
            Stroke::new(1.0, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    } else {
        // Gradually return to no skew when not dragging
        ui_state.skew.x += (0.0 - ui_state.skew.x) * 0.2;
        ui_state.skew.y += (0.0 - ui_state.skew.y) * 0.2;

        let w = note.size.x;
        let h = note.size.y;
        let sx = ui_state.skew.x;
        let sy = ui_state.skew.y;

        let p1 = note.pos;
        let p2 = Pos2 {
            x: note.pos.x + w,
            y: note.pos.y + w * sy,
        };
        let p3 = Pos2 {
            x: note.pos.x + w + h * sx,
            y: note.pos.y + h + w * sy,
        };
        let p4 = Pos2 {
            x: note.pos.x + h * sx,
            y: note.pos.y + h,
        };

        let center = Pos2::new(
            (p1.x + p2.x + p3.x + p4.x) / 4.0,
            (p1.y + p2.y + p3.y + p4.y) / 4.0,
        );

        ui.painter().add(Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            note.color,
            Stroke::NONE,
        ));
        let font_size = fitted_font_size(ui.ctx(), &note.text, note.size, 16.0);
        if highlight_match {
            let job = highlighted_layout(&note.text, query, font_size);
            let galley = ui.painter().layout_job(job);
            ui.painter()
                .galley(center - galley.size() * 0.5, galley, Color32::BLACK);
        } else {
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                &note.text,
                egui::FontId::proportional(font_size),
                Color32::BLACK,
            );
        }
    }

    if highlight_match {
        let stroke = if active {
            Stroke::new(3.0, Color32::RED)
        } else {
            Stroke::new(2.0, Color32::LIGHT_RED)
        };
        ui.painter().rect_stroke(
            Rect::from_min_size(note.pos, note.size),
            0.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    }

    if response.drag_stopped() {
        note.pos = snap_to_grid(note.pos, grid_size);
        if let Some(n) = board.notes.iter_mut().find(|n| n.id == note.id) {
            n.pos = note.pos;
        }
        // Play sound when dragging stops
        ev_plop.write_default();
    }
}

// System to load audio assets at startup
fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(AudioAssets {
        plop: asset_server.load("plop.wav"),
    });
}

// Spawn note entities from the loaded application state
fn spawn_existing_notes(mut commands: Commands, app: Res<PostItData>) {
    for note in &app.state.board.notes {
        commands.spawn((note.clone(), NoteUi::default()));
    }
}
// Auto save when the app exits
fn autosave_on_exit(
    mut exit_events: EventReader<AppExit>,
    mut app: ResMut<PostItData>,
    notes: Query<&NoteData>,
) {
    if exit_events.read().next().is_some() {
        for note in notes.iter() {
            if let Some(n) = app.state.board.notes.iter_mut().find(|n| n.id == note.id) {
                *n = note.clone();
            }
        }
        app.state.save_to_file(&app.save_path);
    }
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.1)))
        .init_resource::<PostItData>()
        .init_resource::<GridSize>()
        .init_resource::<SearchState>()
        .add_event::<PlayPlopEvent>()
        .add_plugins(EntropyPlugin::<WyRand>::default())
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy_egui::EguiPlugin {
            // Default configuration
            enable_multipass_for_primary_context: false,
        })
        .add_systems(Startup, (setup_audio, spawn_existing_notes))
        .add_systems(Update, (ui_system, play_plop_sound))
        .add_systems(Last, autosave_on_exit)
        .run();
}
