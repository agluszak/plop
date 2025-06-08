use bevy::prelude::Component;
use egui::{Color32, Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Data for a single Post-It note
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NoteData {
    pub id: u64,
    pub text: String,
    pub pos: Pos2,
    pub size: Vec2,
    pub color: Color32,
}

/// Virtual board containing multiple notes
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Board {
    pub id: u64,
    pub name: String,
    pub background: Color32,
    pub notes: Vec<NoteData>,
    pub scene_rect: Rect,
}

/// Global application state containing a single board
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AppState {
    pub board: Board,
    pub next_note_id: u64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            board: Board {
                id: 1,
                name: "Board".into(),
                background: Color32::LIGHT_BLUE,
                notes: Vec::new(),
                scene_rect: Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
            },
            next_note_id: 1,
        }
    }
}

impl AppState {
    /// Save to JSON file
    pub fn save_to_file(&self, path: &PathBuf) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Load from JSON file
    pub fn load_from_file(path: &PathBuf) -> Self {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str(&data) {
                return state;
            }
        }
        AppState::default()
    }
}

/// Snap a `Pos2` to the nearest grid cell defined by `grid`.
pub fn snap_to_grid(pos: Pos2, grid: f32) -> Pos2 {
    Pos2::new((pos.x / grid).round() * grid, (pos.y / grid).round() * grid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn save_and_load_roundtrip() {
        let mut state = AppState::default();
        let board = Board {
            id: 1,
            name: "Test".into(),
            background: Color32::WHITE,
            notes: vec![NoteData {
                id: 1,
                text: "hi".into(),
                pos: Pos2 { x: 1.0, y: 2.0 },
                size: Vec2 { x: 10.0, y: 10.0 },
                color: Color32::BLACK,
            }],
            scene_rect: Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
        };
        state.board = board;

        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        state.save_to_file(&path);
        let loaded = AppState::load_from_file(&path);
        assert_eq!(state, loaded);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        drop(file); // delete immediately
        let loaded = AppState::load_from_file(&path);
        assert_eq!(loaded, AppState::default());
    }

    #[test]
    fn load_invalid_json_returns_default() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        fs::write(&path, "not valid json").unwrap();
        let loaded = AppState::load_from_file(&path);
        assert_eq!(loaded, AppState::default());
    }

    #[test]
    fn edited_note_persists_after_save_load() {
        let mut state = AppState::default();
        let mut board = Board {
            id: 1,
            name: "Test".into(),
            background: Color32::WHITE,
            notes: vec![NoteData {
                id: 1,
                text: "hello".into(),
                pos: Pos2 { x: 0.0, y: 0.0 },
                size: Vec2 { x: 10.0, y: 10.0 },
                color: Color32::BLACK,
            }],
            scene_rect: Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
        };
        board.notes[0].text = "edited".into();
        state.board = board.clone();

        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        state.save_to_file(&path);
        let loaded = AppState::load_from_file(&path);
        assert_eq!(loaded.board.notes[0].text, "edited");
        assert_eq!(loaded, state);
    }

    #[test]
    fn snap_to_grid_rounds_position() {
        let pos = Pos2 { x: 27.0, y: 73.0 };
        let snapped = snap_to_grid(pos, 50.0);
        assert_eq!(snapped, Pos2 { x: 50.0, y: 50.0 });
    }
}
