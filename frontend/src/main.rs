mod api;
mod app;
mod components;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!(
        "{} web app is intended for the wasm32 target. Use `dx serve` or build with `--target wasm32-unknown-unknown`.",
        submora_core::APP_NAME
    );
}
