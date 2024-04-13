#!/usr/bin/env sh
set -a; . .env; set +a;

project="bevy_garage_arena_server";

cargo build --release -p $project --target $TARGET --no-default-features --features=headless

rsync -v --progress --copy-links target/$TARGET/release/$project $SERVER_HOST:$SERVER_DIR/

rsync -arvC --progress --copy-links \
    assets/ \
    $SERVER_HOST:$SERVER_DIR/assets/
