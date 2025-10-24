use harmoniq_plugin_bridge::{command_channel, event_channel, BridgeCommand, BridgeServer};

fn main() -> anyhow::Result<()> {
    let server = BridgeServer::new(64);
    let (cmd_tx, cmd_rx) = command_channel();
    let (evt_tx, evt_rx) = event_channel();
    let handle = server.run(cmd_rx, evt_tx);
    cmd_tx.send(BridgeCommand::Create)?;
    cmd_tx.send(BridgeCommand::Process)?;
    cmd_tx.send(BridgeCommand::Destroy)?;
    drop(cmd_tx);
    for event in evt_rx.iter() {
        println!("Event: {:?}", event);
    }
    handle.join().unwrap();
    Ok(())
}
