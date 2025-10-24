use super::{CommandOutcome, ProjectCommand};
use crate::core::state::ProjectState;
use crate::core::CommandError;

struct HistoryEntry {
    label: &'static str,
    command: Box<dyn ProjectCommand>,
    inverse: Box<dyn ProjectCommand>,
}

pub struct CommandBus {
    state: ProjectState,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

impl Default for CommandBus {
    fn default() -> Self {
        Self::new(ProjectState::default())
    }
}

impl CommandBus {
    pub fn new(state: ProjectState) -> Self {
        Self {
            state,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn state(&self) -> &ProjectState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut ProjectState {
        &mut self.state
    }

    pub fn into_state(self) -> ProjectState {
        self.state
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn execute<C>(&mut self, command: C) -> Result<(), CommandError>
    where
        C: ProjectCommand + 'static,
    {
        self.execute_boxed(Box::new(command))
    }

    pub fn execute_boxed(
        &mut self,
        mut command: Box<dyn ProjectCommand>,
    ) -> Result<(), CommandError> {
        let should_merge = self
            .undo_stack
            .last()
            .map(|entry| command.should_merge(&*entry.command))
            .unwrap_or(false);
        if should_merge {
            if let Some(mut entry) = self.undo_stack.pop() {
                let outcome = entry.inverse.apply(&mut self.state)?;
                drop(outcome.inverse);
            }
        }

        let label = command.label();
        let outcome = command.apply(&mut self.state)?;
        if let Err(err) = self.state.ensure_invariants() {
            let revert = outcome.inverse.apply(&mut self.state)?;
            drop(revert.inverse);
            return Err(err);
        }

        self.undo_stack.push(HistoryEntry {
            label,
            command,
            inverse: outcome.inverse,
        });
        self.redo_stack.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> Result<Option<&'static str>, CommandError> {
        let entry = match self.undo_stack.pop() {
            Some(entry) => entry,
            None => return Ok(None),
        };
        let label = entry.label;
        let mut inverse = entry.inverse;
        let outcome = inverse.apply(&mut self.state)?;
        self.redo_stack.push(HistoryEntry {
            label,
            command: outcome.inverse,
            inverse,
        });
        Ok(Some(label))
    }

    pub fn redo(&mut self) -> Result<Option<&'static str>, CommandError> {
        let entry = match self.redo_stack.pop() {
            Some(entry) => entry,
            None => return Ok(None),
        };
        let label = entry.label;
        let mut command = entry.command;
        let outcome = command.apply(&mut self.state)?;
        if let Err(err) = self.state.ensure_invariants() {
            let revert = outcome.inverse.apply(&mut self.state)?;
            drop(revert.inverse);
            return Err(err);
        }
        self.undo_stack.push(HistoryEntry {
            label,
            command,
            inverse: outcome.inverse,
        });
        Ok(Some(label))
    }

    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
