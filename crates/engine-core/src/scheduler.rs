use engine_rt::transport::TransportCommand;
use engine_rt::{EventQueue, QueueError};

#[derive(Clone)]
pub struct RealTimeScheduler {
    transport_queue: EventQueue<TransportCommand>,
}

impl RealTimeScheduler {
    pub fn new(transport_capacity: usize) -> Self {
        Self {
            transport_queue: EventQueue::new(transport_capacity),
        }
    }

    pub fn transport_queue(&self) -> EventQueue<TransportCommand> {
        self.transport_queue.clone()
    }

    pub fn schedule_transport(&self, command: TransportCommand) -> Result<(), QueueError> {
        self.transport_queue.try_push(command)
    }
}
