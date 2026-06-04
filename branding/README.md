# Summon branding

## Structure

```
branding/
├── icon.png                       # App icon mark
├── icon-transparent.png           # App icon (transparent)
├── full-lockup.png                # Icon + wordmark + tagline
├── full-lockup-transparent.png    # Lockup (transparent)
├── wordmark.png                   # Wordmark only
├── wordmark-transparent.png       # Wordmark (transparent)
├── tagline.png                    # Tagline only
├── tagline-transparent.png        # Tagline (transparent)
├── hero-16x9-1600x900.jpg         # README/website hero
├── favicon.ico                    # Multi-size ICO
├── summon-logo.png                # Source logo file
├── Summon.iconset/                # macOS iconset (→ .icns)
├── icons/                         # Sized PNG exports (16–1024px)
└── social/                        # OG image, social cards, avatar
```

## Create a macOS `.icns` file

```sh
cd branding && iconutil -c icns Summon.iconset -o Summon.icns
```

## Notes

Transparent exports are raster approximations. For final branding, convert to vector SVG/Figma.
