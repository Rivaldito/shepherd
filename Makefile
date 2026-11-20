# The native desktop app (native/): build it and run it.
#   make native-build                  build the app and the server it talks to
#   make native-run                    the app on YOUR environment: real HOME (colours, PATH, `claude` stays signed in),
#                                      in its own session `native` that keeps running when the window closes
#   make native-stop                   stop that session
#   make native-desktop                add Shepherd to your applications menu and dock, with its icon (under ~/.local/share)
#   make native-desktop-remove         take it out again
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

.PHONY: native-build native-run native-demo native-shot native-live native-stop native-desktop native-desktop-remove

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

# A desktop entry and an icon, so the menu and the dock show Shepherd with its logo. Under Wayland a window cannot set
# its own icon: the dock finds it through the entry (the app's id is shepherd-native). Only files under ~/.local/share.
DESKTOP_DIR ?= $(HOME)/.local/share/applications
ICON_DIR    ?= $(HOME)/.local/share/icons/hicolor/512x512/apps

native-desktop: native-build
	mkdir -p $(DESKTOP_DIR) $(ICON_DIR)
	cp assets/logo.png $(ICON_DIR)/shepherd-native.png
	printf '[Desktop Entry]\nType=Application\nName=Shepherd\nComment=Teams of coding agents\nExec=$(APP)\nIcon=shepherd-native\nTerminal=false\nCategories=Development;\nStartupWMClass=shepherd-native\n' > $(DESKTOP_DIR)/shepherd-native.desktop
	-update-desktop-database $(DESKTOP_DIR) 2>/dev/null
	-gtk-update-icon-cache -q -t $(HOME)/.local/share/icons/hicolor 2>/dev/null
	@echo "added $(DESKTOP_DIR)/shepherd-native.desktop"

native-desktop-remove:
	rm -f $(DESKTOP_DIR)/shepherd-native.desktop $(ICON_DIR)/shepherd-native.png
