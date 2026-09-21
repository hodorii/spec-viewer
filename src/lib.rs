// Library layers (markdown/watch/ui/app) are built before main.rs wires them,
// so dead-code is expected on lower layers until their upstream tasks land.
#[allow(dead_code)]
pub mod markdown;
#[allow(dead_code)]
pub mod spec;
pub mod watch;
#[allow(dead_code)]
pub mod app;
#[allow(dead_code)]
pub mod ui;
