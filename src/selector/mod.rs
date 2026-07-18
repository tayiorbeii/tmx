pub mod fzf;

use anyhow::Result;

use crate::model::{PaletteRow, UiProfile};

pub trait Selector {
    fn select(&self, rows: &[PaletteRow], profile: UiProfile) -> Result<Option<PaletteRow>>;
}

pub fn default_selector() -> fzf::FzfSelector {
    fzf::FzfSelector::default()
}
