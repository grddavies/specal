ICON_SVG  := assets/icon-macos.svg
ICONSET   := assets/AppIcon.iconset
ICNS      := assets/AppIcon.icns
APP       := target/release/bundle/osx/Specal.app
DMG       := target/release/Specal.dmg
DMG_SVG   := assets/dmg-background.svg
DMG_BG    := target/dmg-background.tiff

.PHONY: help icon app dmg run install-tools clean-icon
.DEFAULT_GOAL := help

help:
	@echo "make icon         generate assets/AppIcon.icns from assets/src/draw_logo.py"
	@echo "make app          build target/release/bundle/osx/Specal.app"
	@echo "make dmg          build $(DMG) with an Applications drop link"
	@echo "make run          build and open the app"
	@echo "make install-tools  install cargo-bundle, librsvg, create-dmg"

$(ICON_SVG): assets/src/draw_logo.py
	python3 assets/src/draw_logo.py

$(ICNS): $(ICON_SVG)
	rm -rf $(ICONSET) && mkdir -p $(ICONSET)
	for s in 16 32 128 256 512; do \
		rsvg-convert -w $$s -h $$s $(ICON_SVG) -o $(ICONSET)/icon_$${s}x$${s}.png; \
		rsvg-convert -w $$((s*2)) -h $$((s*2)) $(ICON_SVG) -o $(ICONSET)/icon_$${s}x$${s}@2x.png; \
	done
	iconutil -c icns $(ICONSET) -o $(ICNS)
	rm -rf $(ICONSET)

icon: $(ICNS)

app: $(ICNS)
	cd gui && cargo bundle --release --format osx
	@echo "built $(APP)"

# 1x + 2x in one tiff so the background is sharp on retina
$(DMG_BG): $(ICON_SVG)
	mkdir -p target
	rsvg-convert -w 540 -h 360 -d 72 -p 72 $(DMG_SVG) -o target/dmg-bg.png
	rsvg-convert -w 1080 -h 720 -d 144 -p 144 $(DMG_SVG) -o target/dmg-bg@2x.png
	tiffutil -cathidpicheck target/dmg-bg.png target/dmg-bg@2x.png -out $(DMG_BG)

dmg: app $(DMG_BG)
	rm -f $(DMG)
	create-dmg \
		--volname "Specal" \
		--volicon $(ICNS) \
		--background $(DMG_BG) \
		--window-size 540 360 \
		--icon-size 112 \
		--icon "Specal.app" 140 170 \
		--app-drop-link 400 170 \
		--hide-extension "Specal.app" \
		$(DMG) $(APP)

run: app
	open $(APP)

install-tools:
	brew install librsvg create-dmg
	cargo install cargo-bundle

clean-icon:
	rm -f $(ICON_SVG) $(ICNS)
