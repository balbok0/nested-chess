use nested_chess::Game;
use wasm_bindgen::prelude::*;

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub fn destination_board(size: u8, square: u8) -> Result<usize, JsValue> {
    nested_chess::route(size, square).map_err(js_error)
}

#[wasm_bindgen]
pub struct NestedGame {
    game: Game,
}

#[wasm_bindgen]
impl NestedGame {
    #[wasm_bindgen(constructor)]
    pub fn new(size: u8) -> Result<NestedGame, JsValue> {
        Ok(Self {
            game: Game::new(size).map_err(js_error)?,
        })
    }
    pub fn chess960(size: u8, index: u16) -> Result<NestedGame, JsValue> {
        Ok(Self {
            game: Game::chess960(size, index).map_err(js_error)?,
        })
    }
    pub fn move_details(&self, board: usize, uci: &str) -> Result<String, JsValue> {
        serde_json::to_string(&self.game.move_details(board, uci).map_err(js_error)?)
            .map_err(js_error)
    }
    pub fn state(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.game.state()).map_err(js_error)
    }
    pub fn legal_moves(&self, board: usize) -> Result<String, JsValue> {
        serde_json::to_string(
            &self
                .game
                .legal_moves(board)
                .map_err(js_error)?
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
        .map_err(js_error)
    }
    pub fn play(&mut self, board: usize, uci: &str) -> Result<String, JsValue> {
        self.game.play(board, uci).map_err(js_error)?;
        self.state()
    }
    pub fn undo(&mut self) -> Result<String, JsValue> {
        self.game.undo().map_err(js_error)?;
        self.state()
    }
    pub fn agree_draw(&mut self, board: usize) -> Result<String, JsValue> {
        self.game.agree_draw(board).map_err(js_error)?;
        self.state()
    }
    pub fn save(&self) -> Result<String, JsValue> {
        self.game.save_json().map_err(js_error)
    }
    pub fn load(json: &str) -> Result<NestedGame, JsValue> {
        Ok(Self {
            game: Game::load_json(json).map_err(js_error)?,
        })
    }
    pub fn piece_planes(&self) -> Vec<u8> {
        self.game.piece_planes()
    }
}
