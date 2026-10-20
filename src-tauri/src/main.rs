// AgentHub desktop bootstrap. All business logic lives in the shared
// `agenthub-core` crate so the CLI and the UI operate on the same engine.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agenthub_lib::run()
}
