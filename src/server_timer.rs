pub struct ServerTimer {
    start_time: std::time::Instant,
}

impl ServerTimer {
    pub fn init() -> ServerTimer {
        ServerTimer {
            start_time: std::time::Instant::now(),
        }
    }

    pub fn get_uptime(&self) -> std::time::Duration {
        std::time::Instant::now() - self.start_time
    }
}
