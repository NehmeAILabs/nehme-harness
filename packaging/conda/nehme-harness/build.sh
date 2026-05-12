#!/bin/bash
set -eux

cargo auditable install --locked --no-track --bins --root "${PREFIX}" --path . --all-features
cargo-bundle-licenses --format yaml --output ./THIRDPARTY.yml
