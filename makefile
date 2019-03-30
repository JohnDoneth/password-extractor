
run:
	cargo build --release
	/usr/bin/time -v target/release/password-extractor -i /home/john/Documents/Password_dataset/BreachCompilation/data -o testing/output

