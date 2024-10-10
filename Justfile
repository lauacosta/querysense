serve:
    sqlx database create
    sqlx migrate run
    cargo run --release

build_client:
    #!/usr/bin/env bash
    set -euxo pipefail
    rm -rf dist/
    cd ./cliente/ && pnpm build && mv dist/ ../
