serve:
    cargo run --release -- serve

sync:
    cargo run --release -- sync
    
clippy:
    cargo clippy -- -Aclippy::pedantic
