PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
CARGO  ?= cargo

.PHONY: build release install uninstall clean

build:
	$(CARGO) build

release:
	$(CARGO) build --release

install: release
	install -Dm755 target/release/wsu $(DESTDIR)$(BINDIR)/wsu
	@echo "installed: $(DESTDIR)$(BINDIR)/wsu"

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/wsu

clean:
	$(CARGO) clean
