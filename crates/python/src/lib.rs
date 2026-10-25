use ::nested_chess::{Game, uci::UciEngine};
use pyo3::{exceptions::PyValueError, prelude::*};

fn py_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pyclass(name = "Game")]
struct PyGame {
    game: Game,
}

#[pymethods]
impl PyGame {
    #[new]
    #[pyo3(signature = (size=2))]
    fn new(size: u8) -> PyResult<Self> {
        Ok(Self {
            game: Game::new(size).map_err(py_error)?,
        })
    }
    fn state(&self) -> PyResult<String> {
        serde_json::to_string(&self.game.state()).map_err(py_error)
    }
    fn legal_moves(&self, board: usize) -> PyResult<Vec<String>> {
        Ok(self
            .game
            .legal_moves(board)
            .map_err(py_error)?
            .iter()
            .map(ToString::to_string)
            .collect())
    }
    fn play(&mut self, board: usize, uci: &str) -> PyResult<String> {
        self.game.play(board, uci).map_err(py_error)?;
        self.state()
    }
    fn undo(&mut self) -> PyResult<String> {
        self.game.undo().map_err(py_error)?;
        self.state()
    }
    fn agree_draw(&mut self, board: usize) -> PyResult<String> {
        self.game.agree_draw(board).map_err(py_error)?;
        self.state()
    }
    fn save(&self) -> PyResult<String> {
        self.game.save_json().map_err(py_error)
    }
    #[staticmethod]
    fn load(json: &str) -> PyResult<Self> {
        Ok(Self {
            game: Game::load_json(json).map_err(py_error)?,
        })
    }
    fn piece_planes(&self) -> Vec<u8> {
        self.game.piece_planes()
    }
}

#[pyclass(unsendable, name = "Opponent")]
struct PyOpponent {
    engine: UciEngine,
}

#[pymethods]
impl PyOpponent {
    #[new]
    fn new(executable: &str, args: Vec<String>, variant_path: &str) -> PyResult<Self> {
        Ok(Self {
            engine: UciEngine::spawn(executable, &args, variant_path).map_err(py_error)?,
        })
    }
    #[pyo3(signature = (game, board, skill=5, milliseconds=200))]
    fn best_move(
        &mut self,
        game: &PyGame,
        board: usize,
        skill: u8,
        milliseconds: u64,
    ) -> PyResult<String> {
        Ok(self
            .engine
            .best_move(&game.game, board, skill, milliseconds)
            .map_err(py_error)?
            .to_string())
    }
}

#[pymodule]
fn nested_chess(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyGame>()?;
    module.add_class::<PyOpponent>()?;
    Ok(())
}
