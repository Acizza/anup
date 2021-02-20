pub mod main_panel;
pub mod prompt;
pub mod series_list;

mod input;

use crate::key::Key;

pub trait Component {
    type State;
    type KeyResult;

    fn process_key(&mut self, _: Key, _: &mut Self::State) -> Self::KeyResult;
}
