#!/usr/bin/env bash

set -e

version=$1

if [[ -z $version ]]; then
  echo "Usage: bump-versions.sh <version>"
  exit 1
fi

cargo_tomls=$(find . -name "Cargo.toml" -not -path "./target/*" -not -path "./test-workspace/*")
for el in $cargo_tomls; do
  cargo release version $version --manifest-path $el -x --no-confirm
done
