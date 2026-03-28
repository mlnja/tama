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
