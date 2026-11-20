# The native desktop app (native/): build it and run it.
#   make native-build                  build the app and the server it talks to
#   make native-run                    the app on YOUR environment: real HOME (colours, PATH, `claude` stays signed in),
#                                      in its own session `native` that keeps running when the window closes
#   make native-stop                   stop that session
#   make native-demo                   a throwaway server with sample data (TAB=agents|tasks|monitor|..., LAUNCH=1)
#   make native-shot SHOT=/tmp/x.png   save a picture of the demo window and exit (PALETTE=text opens the palette)
# Only the demo uses a throwaway HOME, so a shell in ITS terminal has no colours and `claude` asks to sign in.
# That is the demo, not the app: use native-run for real work. Needs ZIG for the terminal engine, e.g.
#   export ZIG=$HOME/.local/opt/zig-x86_64-linux-0.16.0/zig
SERVER ?= $(CURDIR)/target/debug/shepherd
APP    ?= $(CURDIR)/native/target/release/shepherd-native
MODE   ?= shepherd
LIVE_MODE ?= terminal
SHOT   ?= /tmp/shepherd-native.png

.PHONY: native-build native-run native-demo native-shot native-live native-stop

native-build:
	cargo build
	cd native && cargo build --release

# the demo opens on the Shepherd panel (it has the sample data); the real app on the terminal
native-demo: native-build
	MODE=$(MODE) SERVER=$(SERVER) APP=$(APP) bash native/dev/demo.sh

native-shot: native-build
	MODE=$(MODE) SERVER=$(SERVER) APP=$(APP) SHOT=$(SHOT) bash native/dev/demo.sh

# LIVE_MODE is the panel the window opens on: terminal (default) or shepherd
native-run: native-build
	SERVER=$(SERVER) APP=$(APP) MODE=$(LIVE_MODE) bash native/dev/live.sh

# the old name of native-run
native-live: native-run

native-stop:
	SERVER=$(SERVER) bash native/dev/live.sh stop
