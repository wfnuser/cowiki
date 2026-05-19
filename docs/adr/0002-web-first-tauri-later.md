# Web-first, Tauri desktop later

MVP ships as a web application (axum server + React frontend). The desktop version (Tauri 2) reuses the same React frontend and Rust backend code, but is deferred to post-MVP. Web-first is simpler to deploy, demo, and dogfood — one URL for the whole team. The architecture ensures the same Rust business logic powers both surfaces when the desktop version is added.
