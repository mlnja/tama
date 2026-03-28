# Bump the patch version, commit, tag, and push.
release:
    #!/usr/bin/env bash
    set -euo pipefail

    # Read current version from Cargo.toml
    current=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

    # Increment patch
    IFS='.' read -r major minor patch <<< "$current"
    next="$major.$minor.$((patch + 1))"

    echo "Bumping $current → $next"

    # Update Cargo.toml (portable: works on both GNU and BSD sed)
    sed -i.bak "s/^version = \"$current\"/version = \"$next\"/" Cargo.toml
    rm Cargo.toml.bak

    # Update Cargo.lock
    cargo update --workspace --quiet

    git add Cargo.toml Cargo.lock
    git commit -m "chore: bump version to $next"

    git tag -a "v$next" -m "v$next"

    git push origin main
    git push origin "v$next"

    echo "Released v$next"

# Update the Homebrew tap formula with real SHAs for the given release.
# Run this after GitHub Actions has finished publishing the release.
# Usage: just update-tap [version]   (defaults to current Cargo.toml version)
update-tap version="":
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ -z "{{ version }}" ]]; then
        VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    else
        VERSION="{{ version }}"
    fi

    FORMULA="../homebrew-tap/Formula/tama.rb"
    TMPDIR=$(mktemp -d)
    trap "rm -rf $TMPDIR" EXIT

    echo "Updating Homebrew tap for v$VERSION..."

    # Fetch pre-computed .sha256 files from the release assets (tiny, no need to download tarballs)
    for platform in darwin-arm64 darwin-amd64 linux-arm64 linux-amd64; do
        url="https://github.com/mlnja/tama/releases/download/v$VERSION/tama-$platform.tar.gz.sha256"
        sha=$(curl -fsSL "$url" | cut -d' ' -f1)
        echo "  $platform  $sha"
        awk -v sha="$sha" -v marker="# $platform" \
            '$0 ~ marker { sub(/"[0-9a-f]+"/, "\"" sha "\"") } { print }' \
            "$FORMULA" > "$TMPDIR/formula.tmp" && mv "$TMPDIR/formula.tmp" "$FORMULA"
    done

    # Source tarball — GitHub auto-generates this, no .sha256 in release assets, so download it
    echo "  downloading source tarball for SHA..."
    curl -fsSL "https://github.com/mlnja/tama/archive/refs/tags/v$VERSION.tar.gz" \
        -o "$TMPDIR/source.tar.gz"
    source_sha=$(shasum -a 256 "$TMPDIR/source.tar.gz" | cut -d' ' -f1)
    echo "  source  $source_sha"
    awk -v sha="$source_sha" -v marker="# source" \
        '$0 ~ marker { sub(/"[0-9a-f]+"/, "\"" sha "\"") } { print }' \
        "$FORMULA" > "$TMPDIR/formula.tmp" && mv "$TMPDIR/formula.tmp" "$FORMULA"

    # Update version
    sed -i.bak "s/version \"[^\"]*\"/version \"$VERSION\"/" "$FORMULA"
    rm "$FORMULA.bak"

    cd ../homebrew-tap
    git add Formula/tama.rb
    git commit -m "chore: update tama to v$VERSION"
    git push origin main

    echo "Homebrew tap updated for v$VERSION"
