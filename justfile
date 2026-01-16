# Alacritty Math - Development Commands

# Build release version
build:
    cargo build --release

# Start test terminal (builds first, uses isolated config with OSC 133)
test: build
    ZDOTDIR="{{justfile_directory()}}/test-zsh" \
    ./target/release/alacritty \
        --config-file "{{justfile_directory()}}/test-config.toml" \
        --working-directory "{{justfile_directory()}}"

