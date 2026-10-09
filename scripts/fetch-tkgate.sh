#!/bin/zsh
set -euo pipefail
revision=856c40542d3ee5ab0c23cb86a2a4ca0ba623619f
checkout="${1:-/tmp/tkgate-rgate-reference}"
if [[ -d "$checkout/.git" ]]; then
  actual=$(git -C "$checkout" rev-parse HEAD)
  [[ "$actual" == "$revision" ]] || { print -u2 "Unexpected TkGate revision: $actual"; exit 1; }
  print "TkGate reference checkout: $checkout ($revision)"
  exit 0
fi
[[ ! -e "$checkout" ]] || { print -u2 "$checkout exists but is not a git clone; leaving it untouched."; exit 1; }
mkdir -p "${checkout:h}"
git clone https://github.com/bnoordhuis/tkgate.git "$checkout"
git -C "$checkout" checkout --detach "$revision"
print "TkGate reference checkout: $checkout"
