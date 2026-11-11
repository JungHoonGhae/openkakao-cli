#!/bin/sh
# Installs the latest openkakao-cli release for this Mac into ~/.local/bin:
#   curl -fsSL https://raw.githubusercontent.com/JungHoonGhae/openkakao-cli/main/install.sh | sh
# OPENKAKAO_INSTALL_DIR picks another folder, OPENKAKAO_VERSION a release tag (v1.19.2).
# Running it again updates the binary.
set -eu

fail() {
  echo "install.sh: $1" >&2
  exit 1
}

# Everything runs from main, called on the last line, so a download cut short runs nothing.
main() {
  repo="JungHoonGhae/openkakao-cli"
  dir="${OPENKAKAO_INSTALL_DIR:-$HOME/.local/bin}"
  version="${OPENKAKAO_VERSION:-latest}"

  [ "$(uname -s)" = "Darwin" ] || fail "openkakao-cli runs on macOS only."
  # `uname -m` says x86_64 to a shell under Rosetta, so ask the hardware.
  if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ]; then
    target="aarch64-apple-darwin"
  else
    target="x86_64-apple-darwin"
  fi

  asset="openkakao-cli-$target.tar.gz"
  if [ "$version" = "latest" ]; then
    url="https://github.com/$repo/releases/latest/download"
  else
    url="https://github.com/$repo/releases/download/$version"
  fi

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  curl -fsSL "$url/$asset" -o "$tmp/$asset" || fail "could not download $url/$asset"
  curl -fsSL "$url/$asset.sha256" -o "$tmp/$asset.sha256" || fail "could not download the checksum of $asset"
  (cd "$tmp" && shasum -a 256 -c "$asset.sha256" >/dev/null 2>&1) ||
    fail "the checksum of $asset does not match; nothing was installed."

  tar -xzf "$tmp/$asset" -C "$tmp" openkakao-cli
  mkdir -p "$dir"
  install -m 755 "$tmp/openkakao-cli" "$dir/openkakao-cli"
  installed="$("$dir/openkakao-cli" --version)" || fail "$dir/openkakao-cli was installed but does not run."

  echo "Installed $installed at $dir/openkakao-cli"
  case ":$PATH:" in
    *":$dir:"*) echo "Next: openkakao-cli init" ;;
    *)
      echo "$dir is not on your PATH. Add this line to ~/.zshrc, open a new terminal, then run: openkakao-cli init"
      echo "  export PATH=\"$dir:\$PATH\""
      ;;
  esac
}

main
