serve:
    cargo run --quiet --release -- serve

sync:
    cargo run --quiet --release -- sync 

embed input:
    cargo run --release -- embed --model open-ai --input {{ input }}
    
clippy:
    cargo clippy -- -Aclippy::pedantic
