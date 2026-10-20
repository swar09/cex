pub fn snap_shot_worker(mut consumer: EventConsumer) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("snap_shot_worker".to_string())
        .spawn(move || {
            loop {
                // consume the event but only care botu the seqs and discard other thingsF
                // maintain a counter on every 10,000 seqs trigger the snapshot cmd
                // in exchangecommd and update the returned snapshot to redis by
                // snapshot:{symbol}:{time}
            }
        })
        .expect("r")
}
