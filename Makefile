# The native desktop app (native/): build it and try it on a throwaway server (native/dev/demo.sh).
#   make native-build                  build the app and the server it talks to
#   make native-run                    open the window on the demo (TAB=agents|tasks|monitor|..., LAUNCH=1)
#   make native-shot SHOT=/tmp/x.png   save a picture of the window and exit (PALETTE=text opens the palette)
# The demo never touches your own server, sessions or agents. Needs ZIG for the terminal engine, e.g.
#   export ZIG=$HOME/.local/opt/zig-x86_64-linux-0.16.0/zig
SERVER ?= $(CURDIR)/target/debug/shepherd
APP    ?= $(CURDIR)/native/target/release/shepherd-native
MODE   ?= shepherd
SHOT   ?= /tmp/shepherd-native.png

.PHONY: native-build native-run native-shot

native-build:
	cargo build
	cd native && cargo build --release

native-run: native-build
	MODE=$(MODE) SERVER=$(SERVER) APP=$(APP) bash native/dev/demo.sh

native-shot: native-build
	MODE=$(MODE) SERVER=$(SERVER) APP=$(APP) SHOT=$(SHOT) bash native/dev/demo.sh
