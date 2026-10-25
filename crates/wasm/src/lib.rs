use nested_chess::Game;
use nested_chess::search::{self, Evaluator, LocalAnalysis, SearchError, SearchLimits};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise};

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

struct JsEvaluator(js_sys::Function);

impl Evaluator for JsEvaluator {
    async fn evaluate(&mut self, fen: &str, chess960: bool) -> Result<LocalAnalysis, SearchError> {
        let response = self
            .0
            .call2(
                &JsValue::UNDEFINED,
                &JsValue::from_str(fen),
                &JsValue::from_bool(chess960),
            )
            .map_err(|error| SearchError::Evaluation(format!("{error:?}")))?;
        let value = JsFuture::from(js_sys::Promise::resolve(&response))
            .await
            .map_err(|error| SearchError::Evaluation(format!("{error:?}")))?;
        serde_json::from_str(
            &value
                .as_string()
                .ok_or_else(|| SearchError::Evaluation("expected JSON local analysis".into()))?,
        )
        .map_err(|error| SearchError::Evaluation(error.to_string()))
    }
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
    pub fn best_move(
        &self,
        limits: &str,
        evaluate: js_sys::Function,
    ) -> Result<js_sys::Promise, JsValue> {
        let game = self.game.clone();
        let limits: SearchLimits = serde_json::from_str(limits).map_err(js_error)?;
        Ok(future_to_promise(async move {
            let result = search::best_move(&game, &mut JsEvaluator(evaluate), limits)
                .await
                .map_err(js_error)?;
            Ok(JsValue::from_str(
                &serde_json::to_string(&result).map_err(js_error)?,
            ))
        }))
    }
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
    pub fn chess960_boards(size: u8, indices: &[u16]) -> Result<NestedGame, JsValue> {
        Ok(Self {
            game: Game::chess960_boards(size, indices).map_err(js_error)?,
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
