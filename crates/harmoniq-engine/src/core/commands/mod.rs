mod arrangement;
mod automation;
mod bus;
mod mixer;

pub use arrangement::{AddClipCommand, CreateTrackCommand, MoveClipCommand};
pub use automation::WriteAutomationPointCommand;
pub use bus::CommandBus;
pub use mixer::{MixerEndpoint, SetMixerTargetCommand};

use std::any::Any;

use crate::core::state::ProjectState;
use crate::core::CommandError;

pub trait ProjectCommand: Send + Sync + 'static {
    fn label(&self) -> &'static str;
    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError>;
    fn should_merge(&self, _previous: &dyn ProjectCommand) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any;
}

pub struct CommandOutcome {
    pub inverse: Box<dyn ProjectCommand>,
}
