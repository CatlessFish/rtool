cargo fmt -q
set -e
cargo install --path .
cargo rtool -help