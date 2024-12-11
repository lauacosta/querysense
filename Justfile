_default:
    just --list

serve:
    cargo run --quiet --release -- serve

udeps:
    cargo udeps --all-targets --backend depinfo

all: assets serve

assets:
    bun vite build

sync:
    cargo run --quiet --release -- sync 

embed input:
    cargo run --release -- embed --model open-ai --input {{ input }}
    
clippy:
    cargo clippy -- -Aclippy::pedantic
