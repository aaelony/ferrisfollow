
run:
	cargo run

release:
	cargo build --release &&  cp target/release/ferrisfollow ~/bin
