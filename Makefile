
run:
	cargo run

release:
	cargo build --release &&  cp target/release/ferrisfollow ~/bin

flamegraph:
	perf --version
	cargo flamegraph --release --no-inline
