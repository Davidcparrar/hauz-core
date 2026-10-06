//! Read-only terminal bill browser: pure state (`App`), rendering (`draw`) and the DB
//! loader (`load`). The terminal loop lives in the untested `main.rs` shell.

mod app;
mod load;
mod ui;

pub use app::{App, Flow};
pub use load::load;
pub use ui::draw;
