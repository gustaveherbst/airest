.PHONY: dev build install test validate new openapi serve

BINARY := target/release/airest

dev:
	cargo run -- serve

build:
	cargo build --release

install: build
	@echo "Built $(BINARY)"
	@echo "Add to PATH or copy: sudo cp $(BINARY) /usr/local/bin/airest"

start: build
	$(BINARY) serve

serve: start

test:
	cargo test

validate: build
	$(BINARY) validate --folder ./examples

new: build
	$(BINARY) new $(NAME)

openapi: build
	$(BINARY) openapi --output openapi.json
