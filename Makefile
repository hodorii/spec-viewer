.PHONY: build test run install clean install-remote

# Root path passed to `m` for the `run` target; defaults to the parent
# directory (this crate normally lives one level under the workspace whose
# .kiro/ it views).
ROOT ?= ..

build:
	cargo build

test:
	cargo test

run:
	cargo run --bin m -- $(ROOT)

install:
	cargo install --path . --root ~/.local

clean:
	cargo clean

# Build and install on a remote host that has cargo: make install-remote HOST=myhost [REMOTE_DIR=~/dev/tools/spec-viewer]
REMOTE_DIR ?= ~/dev/tools/spec-viewer
install-remote:
	@test -n "$(HOST)" || { echo "usage: make install-remote HOST=<ssh-host>"; exit 2; }
	ssh $(HOST) 'mkdir -p $(REMOTE_DIR)'
	rsync -az --delete --exclude target --exclude .omo ./ $(HOST):$(REMOTE_DIR)/
	ssh $(HOST) '. ~/.cargo/env 2>/dev/null; cd $(REMOTE_DIR) && cargo install --path . --root ~/.local && ~/.local/bin/m --help | head -1'
