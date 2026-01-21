# Alacritty Math - Development Commands

# Build release version
build:
    cargo build --release

# Start test terminal (builds first, uses isolated config with OSC 133)
test: build
    #!/usr/bin/env bash
    tmpdir=$(mktemp -d)
    cat > "$tmpdir/.zshrc" << 'EOF'
    # OSC 133 shell integration
    precmd() { print -Pn "\e]133;A\a" }
    preexec() { print -Pn "\e]133;C\a" }
    PS1='%F{green}[math]%f %~ %# '
    EOF
    ZDOTDIR="$tmpdir" ./target/release/alacritty \
        --config-file "{{justfile_directory()}}/test-config.toml" \
        --working-directory "{{justfile_directory()}}"
    rm -rf "$tmpdir"

