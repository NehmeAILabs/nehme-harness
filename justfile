# Justfile
# https://github.com/casey/just

[private]
default:
    @just --list

# ---- Build ----

build:
    cargo build --release

build-all:
    cargo build --release --all-features

run *args:
    cargo run -- {{ args }}

# ---- Quality ----

fmt:
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings

check:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings

test: fmt
    cargo test

# ---- Git hooks ----

install-hook:
    #!/usr/bin/env bash
    cat > .git/hooks/pre-commit << 'EOF'
    #!/bin/sh
    set -e
    echo "Running pre-commit quality checks..."
    just check
    EOF
    chmod +x .git/hooks/pre-commit
    echo "Pre-commit hook installation confirmed."

remove-hook:
    rm .git/hooks/pre-commit
    echo "Pre-commit hook uninstallation confirmed."

# ---- Tags ----

add-tag:
    #!/usr/bin/env bash
    set -euo pipefail
    git push origin dev
    VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
    git tag -a "v${VERSION}" -m "Release v${VERSION}"
    git push origin "v${VERSION}"
    echo "Created and pushed tag v${VERSION}"

remove-tag VERSION="":
    #!/usr/bin/env bash
    set -e
    tag="{{ VERSION }}"
    if [ -z "$tag" ]; then
        tag=$(git tag | sort -V | fzf --prompt="Select tag to remove: ")
    fi
    if [ -z "$tag" ]; then
        echo "No tag selected"
        exit 1
    fi
    git tag -d "$tag" || {
        echo "Local tag not found"
        exit 1
    }
    git push --delete origin "$tag"
    echo "Removed tag $tag"

# ---- Packaging: version sync ----

# Sync version from Cargo.toml to all packaging files
sync-version:
    bash scripts/sync-version.sh

# ---- Packaging: checksums ----

# Download release artifacts and update AUR PKGBUILD checksums
aur-checksums:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
    echo "Computing SHA256 sums for v${VERSION}..."

    SHA_X86=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-x86_64-unknown-linux-musl.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_AARCH64=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-aarch64-unknown-linux-musl.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_LICENSE=$(curl -sL "https://raw.githubusercontent.com/NehmeAILabs/nehme-harness/v${VERSION}/LICENSE" | sha256sum | cut -d' ' -f1)

    sed -i "s/sha256sums_x86_64=('.*' '.*')/sha256sums_x86_64=('${SHA_X86}' '${SHA_LICENSE}')/" packaging/aur/PKGBUILD
    sed -i "s/sha256sums_aarch64=('.*' '.*')/sha256sums_aarch64=('${SHA_AARCH64}' '${SHA_LICENSE}')/" packaging/aur/PKGBUILD

    echo "Updated sha256sums in packaging/aur/PKGBUILD"

# Update the source tarball SHA256 in conda/nehme-harness/meta.yaml
conda-source-sha256:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
    SHA=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/archive/refs/tags/v${VERSION}.tar.gz" | sha256sum | cut -d' ' -f1)
    sed -i "/^  url:.*archive\/refs\/tags/{n;s/sha256: .*/sha256: ${SHA}/}" packaging/conda/nehme-harness/meta.yaml
    echo "Updated source SHA256 in packaging/conda/nehme-harness/meta.yaml"

# Download release artifacts and update conda/nehme-harness-bin/meta.yaml checksums
conda-bin-checksums:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
    echo "Computing SHA256 sums for v${VERSION}..."

    SHA_X86=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-x86_64-unknown-linux-musl.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_AARCH64=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-aarch64-unknown-linux-musl.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_LICENSE=$(curl -sL "https://raw.githubusercontent.com/NehmeAILabs/nehme-harness/v${VERSION}/LICENSE" | sha256sum | cut -d' ' -f1)

    sed -i "/nh-x86_64-unknown-linux-musl.tar.gz/{n;s/sha256: .*/sha256: ${SHA_X86}/}" packaging/conda/nehme-harness-bin/meta.yaml
    sed -i "/nh-aarch64-unknown-linux-musl.tar.gz/{n;s/sha256: .*/sha256: ${SHA_AARCH64}/}" packaging/conda/nehme-harness-bin/meta.yaml
    sed -i "/raw.githubusercontent.com.*LICENSE/{n;s/sha256: .*/sha256: ${SHA_LICENSE}/}" packaging/conda/nehme-harness-bin/meta.yaml

    echo "Updated SHA256 sums in packaging/conda/nehme-harness-bin/meta.yaml"

# Download release artifacts and update packaging/homebrew/nehme-harness.rb checksums
homebrew-checksums:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
    echo "Computing SHA256 sums for v${VERSION}..."

    SHA_DARWIN_X86=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-x86_64-apple-darwin.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_DARWIN_ARM=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-aarch64-apple-darwin.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_LINUX_X86=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-x86_64-unknown-linux-musl.tar.gz" | sha256sum | cut -d' ' -f1)
    SHA_LINUX_ARM=$(curl -sL "https://github.com/NehmeAILabs/nehme-harness/releases/download/v${VERSION}/nh-aarch64-unknown-linux-musl.tar.gz" | sha256sum | cut -d' ' -f1)

    sed -i "/nh-x86_64-apple-darwin.tar.gz/{n;s/sha256 \".*\"/sha256 \"${SHA_DARWIN_X86}\"/}" packaging/homebrew/nehme-harness.rb
    sed -i "/nh-aarch64-apple-darwin.tar.gz/{n;s/sha256 \".*\"/sha256 \"${SHA_DARWIN_ARM}\"/}" packaging/homebrew/nehme-harness.rb
    sed -i "/nh-x86_64-unknown-linux-musl.tar.gz/{n;s/sha256 \".*\"/sha256 \"${SHA_LINUX_X86}\"/}" packaging/homebrew/nehme-harness.rb
    sed -i "/nh-aarch64-unknown-linux-musl.tar.gz/{n;s/sha256 \".*\"/sha256 \"${SHA_LINUX_ARM}\"/}" packaging/homebrew/nehme-harness.rb

    echo "Updated SHA256 sums in packaging/homebrew/nehme-harness.rb"

# ---- Packaging: AUR metadata ----

# Regenerate .SRCINFO from PKGBUILD (requires makepkg)
aur-regen-srcinfo:
    #!/usr/bin/env bash
    set -euo pipefail
    cd packaging/aur
    makepkg --printsrcinfo > .SRCINFO
    echo "Regenerated packaging/aur/.SRCINFO"

# ---- Packaging: release workflow ----

# Run after bumping Cargo.toml version (syncs version strings, no network needed)
pre-release: sync-version
    @echo "=== pre-release done: version synced across all packaging files ==="
    @echo "Next: just add-tag, wait for GitHub release, then: just post-release"

# Run after the GitHub release has been published (needs tag archive + binaries to be available)
post-release: conda-source-sha256 aur-checksums conda-bin-checksums homebrew-checksums aur-regen-srcinfo
    @echo "=== post-release done: all checksums updated + .SRCINFO regenerated ==="
    @echo "Ready for:"
    @echo "  AUR: cd packaging/aur && pkgctl aur publish nehme-harness-bin"
    @echo "  conda: submit PR to conda-forge/staged-recipes"
    @echo "  homebrew: push packaging/homebrew/nehme-harness.rb to homebrew-tap repo"
