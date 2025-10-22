use engine_rt::queue::QueueError;
use engine_rt::EventQueue;

#[test]
fn push_and_pop() {
    let queue = EventQueue::new(2);
    queue.try_push(1usize).unwrap();
    queue.try_push(2usize).unwrap();
    assert!(matches!(queue.try_push(3usize), Err(QueueError::Full)));
    assert_eq!(queue.try_pop().unwrap(), 1);
    assert_eq!(queue.try_pop().unwrap(), 2);
    assert!(matches!(queue.try_pop(), Err(QueueError::Empty)));
}
