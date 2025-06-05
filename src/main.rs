use bevy::prelude::*;
use bevy_egui::EguiContexts;
use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Resource)]
struct GridConfig {
    size: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self { size: 40.0 }
    }
}

fn snap_to_grid(pos: Pos2, grid: &GridConfig) -> Pos2 {
    Pos2 {
        x: (pos.x / grid.size).round() * grid.size,
        y: (pos.y / grid.size).round() * grid.size,
    }
}

/// Data for a single Post-It note
#[derive(Component, Serialize, Deserialize, Debug, Clone)]
struct NoteData {
    id: u64,
    text: String,
    pos: Pos2,
    size: Vec2,
    color: Color32,
}

/// Runtime UI state for a note
#[derive(Component)]
struct NoteUi {
    is_editing: bool,
    /// Current scaling applied while dragging for squishy effect
    scale: Vec2,
}

impl Default for NoteUi {
    fn default() -> Self {
        Self {
            is_editing: false,
            scale: Vec2::new(1.0, 1.0),
        }
    }
}

/// Tag component to associate a note entity with a board
#[derive(Component)]
struct BelongsToBoard(u64);

/// Virtual board containing multiple notes
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Board {
    id: u64,
    name: String,
    background: Color32,
    notes: Vec<NoteData>,
    scene_rect: Rect,
}

/// Global application state
#[derive(Serialize, Deserialize, Debug, Default)]
struct AppState {
    boards: HashMap<u64, Board>,
    current_board: Option<u64>,
    next_note_id: u64,
    next_board_id: u64,
}

impl AppState {
    /// Save to JSON file
    fn save_to_file(&self, path: &PathBuf) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Load from JSON file
    fn load_from_file(path: &PathBuf) -> Self {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str(&data) {
                return state;
            }
        }
        AppState::default()
    }
}

// Audio resource to play the plop sound
#[derive(Resource)]
struct AudioAssets {
    plop: Handle<AudioSource>,
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

#[derive(Resource)]
struct ActiveBoard(Option<u64>);

// System to handle plop sound events
fn play_plop_sound(
    audio_assets: Res<AudioAssets>,
    mut commands: Commands,
    mut events: EventReader<PlayPlopEvent>,
) {
    for _ in events.read() {
        // Play sound with Bevy's audio systems
        commands.spawn(AudioPlayer::new(audio_assets.plop.clone()));
    }
}

fn ui_system(
    mut commands: Commands,
    mut app: ResMut<PostItData>,
    mut contexts: EguiContexts,
    mut active_board: ResMut<ActiveBoard>,
    mut ev_plop: EventWriter<PlayPlopEvent>,
    mut notes: Query<(Entity, &mut NoteData, &mut NoteUi, &BelongsToBoard)>,
    grid: Res<GridConfig>,
) {
    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Button: Add new board
            if ui.button("New Board").clicked() {
                let id = app.state.next_board_id;
                app.state.next_board_id += 1;
                let board = Board {
                    id,
                    name: format!("Board {}", id),
                    background: Color32::LIGHT_BLUE,
                    notes: Vec::new(),
                    scene_rect: Rect::ZERO,
                };
                app.state.current_board = Some(id);
                active_board.0 = Some(id);
                app.state.boards.insert(id, board);
            }

            // Save/Load controls
            if ui.button("Save").clicked() {
                // Sync notes from ECS into the app state before saving
                for (_, note, _, belongs) in notes.iter_mut() {
                    if let Some(board) = app.state.boards.get_mut(&belongs.0) {
                        if let Some(n) = board.notes.iter_mut().find(|n| n.id == note.id) {
                            *n = note.clone();
                        }
                    }
                }
                app.state.save_to_file(&app.save_path);
            }
            if ui.button("Load").clicked() {
                app.state = AppState::load_from_file(&app.save_path);
                active_board.0 = app.state.current_board;
                // Remove existing note entities
                for (e, _, _, _) in notes.iter_mut() {
                    commands.entity(e).despawn();
                }
                // Spawn notes from loaded state
                for board in app.state.boards.values() {
                    for note in &board.notes {
                        commands.spawn((note.clone(), NoteUi::default(), BelongsToBoard(board.id)));
                    }
                }
            }

            // Board selection dropdown
            if !app.state.boards.is_empty() {
                let current = app.state.current_board.unwrap_or(0);
                let mut selection = current;
                ui.label("Board:");
                egui::ComboBox::new("board_select", "")
                    .selected_text(
                        app.state
                            .boards
                            .get(&selection)
                            .map(|b| b.name.clone())
                            .unwrap_or_default(),
                    )
                    .show_ui(ui, |ui| {
                        for (&id, board) in &app.state.boards {
                            ui.selectable_value(&mut selection, id, &board.name);
                        }
                    });
                app.state.current_board = Some(selection);
                active_board.0 = Some(selection);
            }
        });
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        if let Some(board_id) = active_board.0 {
            let mut next_id = app.state.next_note_id;
            if let Some(board) = app.state.boards.get_mut(&board_id) {
                board_ui_system(
                    ui,
                    board,
                    board_id,
                    &mut next_id,
                    &mut notes,
                    &mut commands,
                    &mut ev_plop,
                    &grid,
                );
            }
            app.state.next_note_id = next_id;
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Create a new board to get started!");
            });
        }
    });
}

/// Render a single board: background + draggable notes
fn board_ui_system(
    ui: &mut egui::Ui,
    board: &mut Board,
    board_id: u64,
    next_note_id: &mut u64,
    notes: &mut Query<(Entity, &mut NoteData, &mut NoteUi, &BelongsToBoard)>,
    commands: &mut Commands,
    ev_plop: &mut EventWriter<PlayPlopEvent>,
    grid: &GridConfig,
) {
    // Allocate the whole available space for our board area
    let board_rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(board_rect, egui::Sense::click_and_drag());

    // Paint the background
    ui.painter().rect_filled(board_rect, 0.0, board.background);

    // Render existing notes from ECS
    for (_, mut note, mut ui_state, belongs) in notes.iter_mut() {
        if belongs.0 == board_id {
            add_note_ui(ui, &mut note, &mut ui_state, board, ev_plop, grid);
        }
    }

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
            pos: pointer_pos,
            size: Vec2 { x: 120.0, y: 80.0 },
            color: Color32::YELLOW,
        };
        commands.spawn((data.clone(), NoteUi::default(), BelongsToBoard(board_id)));
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
    ev_plop: &mut EventWriter<PlayPlopEvent>,
    grid: &GridConfig,
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
                if ui.button("Done").clicked() {
                    ui_state.is_editing = false;
                }
            });
        if let Some(n) = board.notes.iter_mut().find(|n| n.id == note.id) {
            n.text = note.text.clone();
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

        let snapped = snap_to_grid(note.pos, grid);
        let preview_rect = Rect::from_min_size(snapped, note.size);
        ui.painter().rect_stroke(
            preview_rect,
            4.0,
            Stroke::new(1.0, Color32::LIGHT_GRAY),
            StrokeKind::Outside,
        );

        // Update temporary scaling based on drag speed
        let stretch_factor = 0.03;
        let target_scale_x = 1.0 + delta.x.abs() * stretch_factor;
        let target_scale_y = 1.0 + delta.y.abs() * stretch_factor;
        ui_state.scale.x += (target_scale_x - ui_state.scale.x) * 0.5;
        ui_state.scale.y += (target_scale_y - ui_state.scale.y) * 0.5;

        let scaled_size = Vec2::new(
            note.size.x * ui_state.scale.x,
            note.size.y * ui_state.scale.y,
        );
        let mut drag_rect = Rect::from_min_size(note.pos, scaled_size);
        drag_rect = drag_rect.translate(egui::vec2(wiggle_off, 0.0));

        ui.painter().rect_filled(drag_rect, 4.0, note.color);
        ui.painter().text(
            drag_rect.center(),
            egui::Align2::CENTER_CENTER,
            &note.text,
            egui::FontId::proportional(16.0),
            Color32::BLACK,
        );
    } else {
        // Gradually return to original scale when not dragging
        ui_state.scale.x += (1.0 - ui_state.scale.x) * 0.2;
        ui_state.scale.y += (1.0 - ui_state.scale.y) * 0.2;

        let scaled_size = Vec2::new(
            note.size.x * ui_state.scale.x,
            note.size.y * ui_state.scale.y,
        );
        let display_rect = Rect::from_min_size(note.pos, scaled_size);

        ui.painter().rect_filled(display_rect, 4.0, note.color);
        ui.painter().text(
            display_rect.center(),
            egui::Align2::CENTER_CENTER,
            &note.text,
            egui::FontId::proportional(16.0),
            Color32::BLACK,
        );
    }

    if response.drag_stopped() {
        note.pos = snap_to_grid(note.pos, grid);
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
    for board in app.state.boards.values() {
        for note in &board.notes {
            commands.spawn((note.clone(), NoteUi::default(), BelongsToBoard(board.id)));
        }
    }
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.1)))
        .init_resource::<PostItData>()
        .init_resource::<GridConfig>()
        .insert_resource(ActiveBoard(None))
        .add_event::<PlayPlopEvent>()
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy_egui::EguiPlugin {
            // Default configuration
            enable_multipass_for_primary_context: false,
        })
        .add_systems(Startup, (setup_audio, spawn_existing_notes))
        .add_systems(Update, (ui_system, play_plop_sound))
        .run();
}
