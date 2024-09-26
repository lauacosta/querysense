serve:
    sqlx database create
    sqlx migrate run
    docker compose up

build:
    docker compose up --build

build_client:
    #!/usr/bin/env bash
    set -euxo pipefail
    cd ./cliente/ && pnpm build && mv dist/ ../
