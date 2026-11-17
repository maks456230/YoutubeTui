PREFIX ?= $(HOME)/.local
BINS   := ytm ytt

.PHONY: build release install test live-test screenshot clean

build:
	cargo build

release:
	cargo build --release

install: release
	@for b in $(BINS); do \
		install -Dm755 target/release/$$b $(DESTDIR)$(PREFIX)/bin/$$b; \
		echo "установлено: $(DESTDIR)$(PREFIX)/bin/$$b"; \
	done

test:
	cargo test

live-test:
	cargo test -- --ignored

# пересобрать docs/halfblocks.png из текущей вёрстки
screenshot:
	cargo test dump_frame_json -- --ignored --nocapture
	python3 docs/render_frame.py /tmp/ytmt_frame.json docs/halfblocks.png

clean:
	cargo clean
