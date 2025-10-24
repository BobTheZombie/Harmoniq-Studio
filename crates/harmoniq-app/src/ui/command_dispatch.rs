use std::sync::Arc;

use crossbeam_queue::ArrayQueue;

use super::commands::Command;

#[derive(Clone)]
pub struct CommandSender {
    queue: Arc<ArrayQueue<Command>>,
}

impl CommandSender {
    pub fn send(&self, command: Command) -> Result<(), Command> {
        self.queue.push(command)
    }

    pub fn try_send(&self, command: Command) -> Result<(), Command> {
        self.queue.push(command)
    }
}

pub struct CommandReceiver {
    queue: Arc<ArrayQueue<Command>>,
}

impl CommandReceiver {
    fn new(queue: Arc<ArrayQueue<Command>>) -> Self {
        Self { queue }
    }

    pub fn try_recv(&self) -> Option<Command> {
        self.queue.pop().ok()
    }
}

pub fn command_channel(capacity: usize) -> (CommandSender, CommandReceiver) {
    let queue = Arc::new(ArrayQueue::new(capacity));
    (
        CommandSender {
            queue: queue.clone(),
        },
        CommandReceiver::new(queue),
    )
}

pub trait CommandHandler {
    fn handle_command(&mut self, command: Command);
}

pub struct CommandDispatcher {
    receiver: CommandReceiver,
}

impl CommandDispatcher {
    pub fn new(receiver: CommandReceiver) -> Self {
        Self { receiver }
    }

    pub fn poll(&mut self, handler: &mut impl CommandHandler) {
        while let Some(command) = self.receiver.try_recv() {
            handler.handle_command(command);
        }
    }
}
