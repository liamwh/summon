# Summon CLI - justfile commands

# Show available commands
default:
    @just --list --justfile {{ justfile() }}

# Build the summon binary in debug mode
build:
    cargo build

# Build the summon binary in release mode
build-release:
    cargo build --release

# Run tests
test:
    cargo nextest run

# Run clippy lints
lint:
    cargo clippy --all-targets --all-features --workspace --quiet --no-deps

# Format all Rust code in the workspace
format:
    cargo fmt --all

# Install summon CLI to ~/bin (backs up existing binary with timestamp)
install:
    @echo "📦 Building summon binary..."
    cargo build --release -q --target-dir target
    @echo "📥 Installing to ~/bin..."
    @mkdir -p ~/bin
    @mkdir -p ~/bin/.summon-backups
    @if [ -f ~/bin/summon ]; then \
        backup=~/bin/.summon-backups/summon.$(date +%Y%m%d-%H%M%S); \
        echo "💾 Backing up existing summon to $$backup"; \
        cp ~/bin/summon "$$backup"; \
    fi
    @cp target/release/summon ~/bin/summon
    @echo "🔐 Code signing binary for macOS..."
    @codesign --force --deep -s - ~/bin/summon 2>/dev/null || true
    @echo "✅ summon installed to ~/bin/summon"

# Install Raycast example scripts to ~/.config/raycast/scripts
install-raycast:
    @mkdir -p ~/.config/raycast/scripts
    @cp examples/raycast/*.sh ~/.config/raycast/scripts/
    @echo "✅ Raycast scripts installed to ~/.config/raycast/scripts"

# Clean build artifacts
clean:
    cargo clean
